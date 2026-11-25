//! KernelSU's page: what the phone runs, which KMI its module was built for,
//! and writing the chosen one into boot or init_boot.

use super::*;

/// One module the recovery can write, as the page lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KmiRow {
    /// `android<release>-<kernel>`, as KernelSU names it.
    pub id: String,
    pub android: u8,
    pub kernel: String,
    /// The partition it goes into by default.
    pub target: String,
}

/// What the phone and KernelSU's payloads say before anything is written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KernelSuStatus {
    /// KernelSU's userspace was found in the data partition.
    pub installed: bool,
    /// The stock image this recovery wrote KernelSU over is still there.
    pub has_backup: bool,
    /// The partition the record's backup came from.
    pub backup_target: Option<String>,
    pub version: Option<String>,
    /// The bootloader lets its boot images be written.
    pub unlocked: bool,
    /// The slot whose boot image is written, where the phone has slots.
    pub slot: Option<String>,
    /// The row the kernel's own version picks, where it picks one.
    pub default: Option<usize>,
    pub kmis: Vec<KmiRow>,
}

/// How far writing KernelSU has come.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KernelSuStage {
    Preparing,
    Patching,
    BackingUp,
    Writing,
    Checking,
}

/// How writing KernelSU ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KernelSuEnd {
    Installed { target: String, backup: String, manager: String },
    Restored { target: String },
    Failed(String),
}

/// Writing KernelSU, or how it last ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KernelSuJob {
    pub stage: KernelSuStage,
    pub outcome: Option<KernelSuEnd>,
    /// The latest release's version, once it has been downloaded.
    pub fetched: Option<String>,
    /// Why the latest release could not be fetched, when the built-in
    /// payloads were used instead.
    pub fallback: Option<String>,
}

impl KernelSuStage {
    pub fn done(self) -> bool {
        matches!(self, KernelSuStage::Checking)
    }
}

impl App {
    /// What the phone and KernelSU's payloads say, read as the page opens.
    pub fn set_kernel_su(&mut self, status: Result<KernelSuStatus, String>) {
        if let Ok(s) = &status
            && self.state.kernel_su_kmi.is_none()
        {
            self.state.kernel_su_kmi = s.default;
        }
        self.state.kernel_su = Some(status);
        self.refresh();
    }

    /// Choose the KMI to write, by its place in the list.
    pub fn set_kernel_su_kmi(&mut self, at: usize) {
        if self.state.kernel_su.as_ref().is_some_and(|s| s.as_ref().is_ok_and(|s| at < s.kmis.len())) {
            self.state.kernel_su_kmi = Some(at);
            self.refresh();
        }
    }

    /// Writing KernelSU has begun.
    pub fn kernel_su_started(&mut self) {
        self.state.kernel_su_job = Some(KernelSuJob { stage: KernelSuStage::Preparing, outcome: None, fetched: None, fallback: None });
        self.refresh();
    }

    /// How far writing KernelSU has come.
    pub fn kernel_su_progress(&mut self, stage: KernelSuStage) {
        let job = self.state.kernel_su_job.get_or_insert(KernelSuJob { stage, outcome: None, fetched: None, fallback: None });
        if job.outcome.is_none() {
            job.stage = stage;
            self.refresh();
        }
    }

    /// The latest release has been downloaded; the page and the toast say
    /// which version is being written.
    pub fn kernel_su_fetched(&mut self, version: String, now: u64) {
        if let Some(job) = &mut self.state.kernel_su_job {
            job.fetched = Some(version.clone());
        }
        let said = format!("{} {version}", self.state.lang.pick(texts::kernel_su::DOWNLOADED));
        self.report(&said, now);
        self.refresh();
    }

    /// The latest release could not be fetched; the built-in payloads are
    /// used, and both the page and the toast say why.
    pub fn kernel_su_fallback(&mut self, why: String, now: u64) {
        if let Some(job) = &mut self.state.kernel_su_job {
            job.fallback = Some(why.clone());
        }
        let said = format!("{} {why}", self.state.lang.pick(texts::kernel_su::FALLBACK));
        self.report(&said, now);
        self.refresh();
    }

    /// Writing KernelSU ended, and the page says how.
    pub fn kernel_su_ended(&mut self, end: KernelSuEnd, now: u64) {
        let job = self.state.kernel_su_job.get_or_insert(KernelSuJob { stage: KernelSuStage::Checking, outcome: None, fetched: None, fallback: None });
        job.outcome = Some(end);
        self.wake(now);
        self.refresh();
    }

    /// A root image was not written, and the toast says why.
    pub fn root_failed(&mut self, why: &str, now: u64) {
        let said = format!("{} {why}", self.state.lang.pick(texts::kernel_su::FAILED));
        self.report(&said, now);
    }

    /// The phone cannot leave while KernelSU is being written.
    pub fn report_kernel_su_busy(&mut self, now: u64) {
        let said = self.state.lang.pick(texts::kernel_su::BUSY);
        self.report(said, now);
    }

    /// The phone cannot leave while Magisk is being written.
    pub fn report_magisk_busy(&mut self, now: u64) {
        let said = self.state.lang.pick(texts::magisk::BUSY);
        self.report(said, now);
    }

    /// The phone cannot leave while an update's snapshots are merging.
    pub fn report_merging(&mut self, now: u64) {
        let said = self.state.lang.pick(texts::update::MERGING);
        self.report(said, now);
    }

    /// The KMI the page would write.
    pub fn kernel_su_kmi(&self) -> Option<&KmiRow> {
        let status = self.state.kernel_su.as_ref()?.as_ref().ok()?;
        let at = self.state.kernel_su_kmi.or(status.default)?;
        status.kmis.get(at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::testing::*;
    use crate::app::{Confirm, Data, Effect};

    fn status() -> KernelSuStatus {
        KernelSuStatus {
            installed: false,
            has_backup: false,
            backup_target: None,
            version: None,
            unlocked: true,
            slot: Some("B".into()),
            default: Some(1),
            kmis: vec![
                KmiRow { id: "android13-5.10".into(), android: 13, kernel: "5.10".into(), target: "boot".into() },
                KmiRow { id: "android17-6.18".into(), android: 17, kernel: "6.18".into(), target: "init_boot".into() },
            ],
        }
    }

    #[test]
    fn the_page_opens_on_the_kernel_s_default_and_takes_another_kmi() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::KernelSu, W, H, PPD);
        app.set_kernel_su(Ok(status()));
        assert_eq!(app.kernel_su_kmi().unwrap().id, "android17-6.18");
        assert_eq!(tap(&mut app, &f, &i, Action::Open(Screen::KernelSuKmi)), vec![Effect::ReadKernelSu]);
        assert_eq!(app.screen(), Screen::KernelSuKmi);
        assert!(tap(&mut app, &f, &i, Action::KernelSuKmi(0)).is_empty());
        assert_eq!(app.screen(), Screen::KernelSu);
        assert_eq!(app.kernel_su_kmi().unwrap().id, "android13-5.10");
    }

    #[test]
    fn writing_the_chosen_kmi_asks_for_it_and_locks_the_page_until_it_ends() {
        let mut app = App::new(Screen::KernelSu, W, H, PPD);
        app.set_kernel_su(Ok(status()));
        app.state.data = Data::Open;
        let fx = app.confirmed(Confirm::KernelSu, 0);
        assert_eq!(fx, vec![Effect::InstallKernelSu { kmi: "android17-6.18".into(), module: None }]);
        assert!(app.state.kernel_su_job.is_some());
        assert!(app.confirmed(Confirm::KernelSu, 0).is_empty(), "the write is not started twice");
    }

    #[test]
    fn a_chosen_local_module_goes_with_the_kmi() {
        let mut app = App::new(Screen::KernelSu, W, H, PPD);
        app.set_kernel_su(Ok(status()));
        app.state.data = Data::Open;
        app.state.kernel_su_ko = Some("/sdcard/Download/kernelsu.ko".into());
        assert_eq!(
            app.confirmed(Confirm::KernelSu, 0),
            vec![Effect::InstallKernelSu {
                kmi: "android17-6.18".into(),
                module: Some("/sdcard/Download/kernelsu.ko".into()),
            }]
        );
    }

    #[test]
    fn a_ko_is_chosen_from_a_place_and_the_built_in_one_can_be_chosen_back() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::KernelSuModule, W, H, PPD);
        app.state.data = Data::Open;
        assert_eq!(
            tap(&mut app, &f, &i, Action::Open(Screen::ChooseKo(Place::Storage))),
            vec![Effect::ReadKoPlace(Place::Storage, String::new())]
        );
        app.set_ko_place(Place::Storage, String::new(), Ok(vec![entry("kernelsu.ko", FileKind::File)]), LISTED);
        let mut c = crate::Canvas::new(W, H);
        app.compose(&mut c, &f, &i, "12:00", 0);
        assert!(tap(&mut app, &f, &i, Action::ChooseKoEntry(0)).is_empty());
        assert_eq!(app.screen(), Screen::KernelSuModule);
        assert_eq!(app.state.kernel_su_ko.as_deref(), Some("kernelsu.ko"));
        assert!(tap(&mut app, &f, &i, Action::KernelSuModuleBuiltIn).is_empty());
        assert_eq!(app.state.kernel_su_ko, None);
    }

    #[test]
    fn a_locked_bootloader_is_refused_before_anything_is_written() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::KernelSu, W, H, PPD);
        let mut status = status();
        status.unlocked = false;
        app.set_kernel_su(Ok(status));
        app.state.data = Data::Open;
        assert!(tap(&mut app, &f, &i, Action::Confirm(Confirm::KernelSu)).is_empty());
        assert!(app.state.kernel_su_job.is_none());
    }

    #[test]
    fn writing_reports_its_stages_and_end_without_losing_them() {
        let mut app = App::new(Screen::KernelSu, W, H, PPD);
        app.kernel_su_started();
        app.kernel_su_progress(KernelSuStage::Patching);
        assert_eq!(app.state.kernel_su_job.as_ref().unwrap().stage, KernelSuStage::Patching);
        app.kernel_su_ended(
            KernelSuEnd::Installed {
                target: "init_boot".into(),
                backup: "/data/media/0/Skyward/root/a.img".into(),
                manager: "/sdcard/Download/Skyward/KernelSU.apk".into(),
            },
            0,
        );
        assert!(matches!(app.state.kernel_su_job.as_ref().unwrap().outcome, Some(KernelSuEnd::Installed { .. })));
    }
}
