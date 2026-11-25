//! The modules Magisk and KernelSU install: a folder each under
//! `/data/adb/modules`, with a `module.prop` in it, a `disable` file when it
//! is off and a `remove` file when it is to go at the next start.
//!
//! Recovery reads and marks them without running any of their own code. A
//! module whose early-boot files are cached, whose newer copy waits to be
//! merged, or that keeps files init's own scripts run, is left alone:
//! changing it here would not take effect, or would take effect halfway.

use anyhow::{ensure, Context, Result};
use skyward_ui::app::ModuleRow;
use std::collections::BTreeMap;
use std::ffi::CString;
use std::fs::File;
use std::io::{ErrorKind, Read};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
#[cfg(target_os = "android")]
use std::process::Command;

#[cfg(target_os = "android")]
const ADB: &str = "/data/adb";
#[cfg(target_os = "android")]
const METADATA: &str = "/metadata";

/// How large a `module.prop` may be before it is not a module's.
const PROP_LIMIT: u64 = 64 * 1024;

#[cfg(target_os = "android")]
pub fn read() -> Result<Vec<ModuleRow>, String> {
    read_in(Path::new(ADB), Path::new(METADATA)).map_err(|e| format!("{e:#}"))
}

/// Install the zip at `path` through whichever root manager is on the
/// phone. The manager runs the module's own installer and stages it where
/// it will be merged at the next start.
#[cfg(target_os = "android")]
pub fn install(path: &str) -> Result<String> {
    let magisk = Path::new("/data/adb/magisk/magisk");
    let ksuds = ["/data/adb/ksu/bin/ksud", "/data/adb/ksud", "/system/bin/ksud"];
    let ksud = ksuds.iter().find_map(|path| {
        let path = Path::new(path);
        path.is_file().then_some(path)
    });
    // The manager whose boot image is patched is the one whose modules the
    // installed system loads, even when the other one's files are still
    // in `/data/adb`.
    match crate::root::installed_root() {
        Some(crate::root::InstalledRoot::KernelSu) => {
            let ksud = ksud.context("KernelSU's ksud is not on the phone")?;
            run(ksud, &["module", "install", path])?;
            return Ok("KernelSU".into());
        }
        Some(crate::root::InstalledRoot::Magisk) => {
            anyhow::ensure!(magisk.is_file(), "Magisk's binary is not on the phone");
            run(magisk, &["--install-module", path])?;
            return Ok("Magisk".into());
        }
        None => {}
    }
    if magisk.is_file() {
        run(magisk, &["--install-module", path])?;
        return Ok("Magisk".into());
    }
    if let Some(ksud) = ksud {
        run(ksud, &["module", "install", path])?;
        return Ok("KernelSU".into());
    }
    anyhow::bail!("neither Magisk nor KernelSU is installed")
}

#[cfg(target_os = "android")]
fn run(program: &Path, args: &[&str]) -> Result<()> {
    let out = Command::new(program).args(args).output().with_context(|| format!("run {}", program.display()))?;
    anyhow::ensure!(
        out.status.success(),
        "{} failed: {}{}",
        program.display(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(())
}

#[cfg(target_os = "android")]
pub enum Event {
    Ended(Result<String, String>),
}

/// Installing a module runs the module's own scripts, so it is done off
/// the thread that draws the pages.
#[cfg(target_os = "android")]
pub struct Worker {
    worker: skyward_sys::worker::Worker<Event>,
}

#[cfg(target_os = "android")]
impl Worker {
    pub fn start(path: String) -> Result<Worker> {
        let worker = skyward_sys::worker::Worker::start("modules", move |tell| {
            let end = install(&path).map_err(|e| format!("{e:#}").chars().take(300).collect::<String>());
            tell.tell(Event::Ended(end));
        })?;
        Ok(Worker { worker })
    }

    pub fn fd(&self) -> std::os::fd::RawFd {
        self.worker.fd()
    }

    pub fn events(&self) -> Vec<Event> {
        self.worker.events()
    }
}

/// A module directory and its properties, read without following links.
struct Dir {
    file: File,
    path: std::path::PathBuf,
}

impl Dir {
    fn open(path: &Path) -> std::io::Result<Self> {
        let file = File::options()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)?;
        Ok(Self { file, path: path.to_path_buf() })
    }

    fn file(&self, name: &str, flags: i32) -> std::io::Result<File> {
        let name = CString::new(name).map_err(std::io::Error::other)?;
        let fd = unsafe { libc::openat(self.file.as_raw_fd(), name.as_ptr(), flags | libc::O_NOFOLLOW | libc::O_CLOEXEC, 0o600) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(unsafe { File::from_raw_fd(fd) })
    }

    fn child(&self, name: &str) -> std::io::Result<Self> {
        let file = self.file(name, libc::O_RDONLY | libc::O_DIRECTORY)?;
        Ok(Self { file, path: self.path.join(name) })
    }

    fn flag(&self, name: &str) -> Result<bool> {
        match self.file(name, libc::O_RDONLY | libc::O_NONBLOCK) {
            Ok(file) => {
                ensure!(file.metadata()?.is_file(), "{name} is not a regular marker file");
                Ok(true)
            }
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e).with_context(|| format!("read module marker {name}")),
        }
    }

    fn mark(&self, name: &str, present: bool) -> Result<()> {
        if self.flag(name)? == present {
            return Ok(());
        }
        if present {
            let file = self.file(name, libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL)?;
            file.sync_all()?;
        } else {
            let name = CString::new(name)?;
            ensure!(
                unsafe { libc::unlinkat(self.file.as_raw_fd(), name.as_ptr(), 0) } == 0,
                "remove marker: {}",
                std::io::Error::last_os_error()
            );
        }
        self.file.sync_all()?;
        Ok(())
    }

    fn properties(&self) -> Result<BTreeMap<String, String>> {
        let file = self.file("module.prop", libc::O_RDONLY | libc::O_NONBLOCK)?;
        ensure!(file.metadata()?.is_file(), "module.prop is not a regular file");
        let mut text = String::new();
        file.take(PROP_LIMIT + 1).read_to_string(&mut text)?;
        ensure!(text.len() as u64 <= PROP_LIMIT, "module.prop is too large");
        Ok(text
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                if line.starts_with('#') {
                    return None;
                }
                let (name, value) = line.split_once('=')?;
                Some((name.trim().to_owned(), value.trim().to_owned()))
            })
            .collect())
    }

    fn names(&self) -> Result<Vec<String>> {
        let mut names = Vec::new();
        #[cfg(target_os = "android")]
        let listing = std::fs::read_dir(format!("/proc/self/fd/{}", self.file.as_raw_fd()))?;
        #[cfg(not(target_os = "android"))]
        let listing = std::fs::read_dir(&self.path)?;
        for entry in listing {
            let entry = entry?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if valid_id(&name) {
                names.push(name);
            }
        }
        Ok(names)
    }
}

/// A module's own name and how its manager will treat it at the next start.
#[derive(Clone, Debug)]
struct Entry {
    name: String,
    note: String,
    about: String,
    enabled: bool,
    removing: bool,
    pending: bool,
    editable: bool,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && id != "." && id != ".." && id.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

fn optional_child(dir: &Dir, name: &str) -> Result<Option<Dir>> {
    match dir.child(name) {
        Ok(dir) => Ok(Some(dir)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("open {name} without following links")),
    }
}

/// A module whose early-boot files are cached may still run after a marker
/// changes. Recovery leaves those caches and the modules they affect alone.
fn has_boot_cache(metadata: &Path) -> Result<bool> {
    for root in ["vold/ksu", "watchdog/ksu", "ksu"] {
        let dir = match Dir::open(&metadata.join(root)) {
            Ok(dir) => dir,
            Err(e) if e.kind() == ErrorKind::NotFound => continue,
            Err(e) => return Err(e).context("open KernelSU boot cache"),
        };
        match dir.file("modules.rc", libc::O_RDONLY | libc::O_NONBLOCK) {
            Ok(file) => {
                let data = file.metadata()?;
                ensure!(data.is_file(), "KernelSU modules.rc is not a regular file");
                // The manager can leave an empty or whitespace-only file;
                // a larger one is an active cache, without reading it all.
                let mut contents = Vec::new();
                file.take(PROP_LIMIT + 1).read_to_end(&mut contents)?;
                if contents.len() as u64 > PROP_LIMIT || contents.iter().any(|byte| !byte.is_ascii_whitespace()) {
                    return Ok(true);
                }
            }
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(e).context("read KernelSU modules.rc"),
        }
        if let Some(preinit) = optional_child(&dir, "preinit")?
            && !preinit.names()?.is_empty()
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Whether the module's markers can take effect at the next start: a
/// staged copy or a cached early-boot state is Android's to finish, and a
/// module whose own files init's scripts run is not ours to turn off.
fn editable(adb: &Path, root: &Dir, dir: &Dir, id: &str, cached: bool) -> Result<bool> {
    if cached || ["ksu/modules.img", "ksu/modules_update.img", "modules.img"].iter().any(|path| adb.join(path).exists()) {
        return Ok(false);
    }
    let pending = match optional_child(root, "modules_update")? {
        Some(staged) => optional_child(&staged, id)?.is_some(),
        None => false,
    };
    Ok(!pending && !dir.flag("update")? && optional_child(dir, "initrc")?.is_none() && optional_child(dir, "preinit")?.is_none())
}

fn read_module(adb: &Path, root: &Dir, dir: &Dir, id: &str, cached: bool) -> Result<Entry> {
    let props = dir.properties()?;
    ensure!(props.get("id").is_some_and(|value| value == id), "module.prop id differs from directory {id}");
    let field = |name: &str| props.get(name).cloned().unwrap_or_default();
    let (version, author) = (field("version"), field("author"));
    let note = match (version.is_empty(), author.is_empty()) {
        (true, true) => String::new(),
        (false, true) => version,
        (true, false) => author,
        (false, false) => format!("{version} · {author}"),
    };
    let pending = match optional_child(root, "modules_update")? {
        Some(staged) => optional_child(&staged, id)?.is_some(),
        None => false,
    };
    Ok(Entry {
        name: {
            let name = field("name");
            if name.is_empty() { id.to_owned() } else { name }
        },
        note,
        about: field("description"),
        enabled: !dir.flag("disable")?,
        removing: dir.flag("remove")?,
        pending,
        editable: editable(adb, root, dir, id, cached)?,
    })
}

fn read_in(adb: &Path, metadata: &Path) -> Result<Vec<ModuleRow>> {
    let root = match Dir::open(adb) {
        Ok(dir) => dir,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).context("open the root managers' directory"),
    };
    let Some(modules) = optional_child(&root, "modules")? else { return Ok(Vec::new()) };
    let cached = has_boot_cache(metadata)?;
    let mut rows = Vec::new();
    for id in modules.names()? {
        // A damaged entry should not prevent the user from managing the
        // others.
        let entry = modules.child(&id).map_err(anyhow::Error::from).and_then(|dir| read_module(adb, &root, &dir, &id, cached));
        if let Ok(module) = entry {
            rows.push(ModuleRow {
                id,
                name: module.name,
                note: module.note,
                about: module.about,
                enabled: module.enabled,
                removing: module.removing,
                pending: module.pending,
                editable: module.editable,
            });
        }
    }
    rows.sort_by_key(|module| module.name.to_lowercase());
    Ok(rows)
}

#[cfg(target_os = "android")]
pub fn switch(id: &str, on: bool) -> Result<()> {
    switch_in(Path::new(ADB), Path::new(METADATA), id, on)
}

fn switch_in(adb: &Path, metadata: &Path, id: &str, on: bool) -> Result<()> {
    ensure!(valid_id(id), "invalid module id");
    let root = Dir::open(adb)?;
    let dir = root.child("modules")?.child(id)?;
    let module = read_module(adb, &root, &dir, id, has_boot_cache(metadata)?)?;
    ensure!(module.editable, "module {id} has an update or early-boot state managed by Android");
    ensure!(!on || !module.removing, "cancel the pending removal before enabling {id}");
    dir.mark("disable", !on).with_context(|| format!("switch {id} {}", if on { "on" } else { "off" }))
}

#[cfg(target_os = "android")]
pub fn remove(id: &str, on: bool) -> Result<()> {
    remove_in(Path::new(ADB), Path::new(METADATA), id, on)
}

fn remove_in(adb: &Path, metadata: &Path, id: &str, on: bool) -> Result<()> {
    ensure!(valid_id(id), "invalid module id");
    let root = Dir::open(adb)?;
    let dir = root.child("modules")?.child(id)?;
    let module = read_module(adb, &root, &dir, id, has_boot_cache(metadata)?)?;
    ensure!(module.editable, "module {id} has an update or early-boot state managed by Android");
    dir.mark("remove", on).with_context(|| format!("{} {id}", if on { "mark for removal" } else { "keep" }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::symlink;

    fn scratch(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let base = std::env::temp_dir().join(format!("skyward-modules-{tag}-{}", std::process::id()));
        fs::remove_dir_all(&base).ok();
        let adb = base.join("adb");
        let metadata = base.join("metadata");
        fs::create_dir_all(adb.join("modules/one")).unwrap();
        fs::write(adb.join("modules/one/module.prop"), "id=one\nname=One\nversion=v1\nauthor=me\ndescription=does one\n").unwrap();
        (adb, metadata)
    }

    #[test]
    fn a_module_is_read_from_its_prop_and_turned_off_by_the_disable_file() {
        let (adb, metadata) = scratch("read");
        let rows = read_in(&adb, &metadata).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].id.as_str(), rows[0].enabled, rows[0].editable), ("one", true, true));
        switch_in(&adb, &metadata, "one", false).unwrap();
        assert!(adb.join("modules/one/disable").exists());
        remove_in(&adb, &metadata, "one", true).unwrap();
        assert!(adb.join("modules/one/remove").exists());
        fs::remove_dir_all(adb.parent().unwrap()).ok();
    }

    #[test]
    fn a_staged_or_cached_module_is_listed_but_not_written() {
        let (adb, metadata) = scratch("staged");
        fs::create_dir_all(adb.join("modules_update/one")).unwrap();
        let rows = read_in(&adb, &metadata).unwrap();
        assert!(rows[0].pending && !rows[0].editable, "a staged copy waits for the next start");
        assert!(switch_in(&adb, &metadata, "one", false).is_err());
        assert!(!adb.join("modules/one/disable").exists());

        fs::remove_dir(adb.join("modules_update/one")).unwrap();
        fs::create_dir_all(metadata.join("vold/ksu")).unwrap();
        fs::write(metadata.join("vold/ksu/modules.rc"), "service demo /example\n").unwrap();
        assert!(!read_in(&adb, &metadata).unwrap()[0].editable, "a cached early-boot module is Android's");
        assert!(remove_in(&adb, &metadata, "one", true).is_err());
        fs::remove_dir_all(adb.parent().unwrap()).ok();
    }

    #[test]
    fn module_directories_and_marker_files_do_not_follow_symlinks() {
        let (adb, metadata) = scratch("links");
        let base = adb.parent().unwrap();
        let outside = base.join("outside");
        fs::write(&outside, "keep").unwrap();
        symlink(&outside, adb.join("modules/one/disable")).unwrap();
        assert!(switch_in(&adb, &metadata, "one", true).is_err(), "a marker that is a link is not read or removed");
        assert_eq!(fs::read_to_string(&outside).unwrap(), "keep");
        symlink(adb.join("modules/one"), adb.join("modules/linked")).unwrap();
        assert!(read_in(&adb, &metadata).unwrap().iter().all(|module| module.id != "linked"), "a module directory that is a link is not listed");
        assert!(switch_in(&adb, &metadata, "../outside", false).is_err(), "an id is a name, not a path");
        fs::remove_dir_all(base).ok();
    }
}
