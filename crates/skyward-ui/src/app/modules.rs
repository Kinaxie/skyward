//! The modules the phone's own system loads: Magisk's and KernelSU's keep
//! them the same way, each in a folder under `/data/adb/modules`.

use super::*;

/// One module the page lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleRow {
    /// The folder's name, which is how the module is known.
    pub id: String,
    pub name: String,
    /// Its version and author, as `module.prop` writes them.
    pub note: String,
    pub about: String,
    pub enabled: bool,
    /// Marked to be removed the next time the phone starts.
    pub removing: bool,
    /// A newer copy waits in `modules_update` for the next start.
    pub pending: bool,
    /// Its markers can take effect at the next start; a module with a
    /// staged copy or an early-boot state managed by Android cannot.
    pub editable: bool,
}

impl App {
    /// The modules read for the page, or why they could not be.
    pub fn modules(&self) -> Option<&Result<Vec<ModuleRow>, String>> {
        self.state.modules.as_ref()
    }

    /// The modules read, as the page shows them.
    pub fn set_modules(&mut self, rows: Result<Vec<ModuleRow>, String>) {
        self.state.modules = Some(rows);
        self.refresh();
    }

    /// Open the details of the module at this place in the list.
    pub fn open_module(&mut self, at: usize) {
        if self.state.modules.as_ref().is_some_and(|r| r.as_ref().is_ok_and(|list| at < list.len())) {
            self.state.module_at = Some(at);
            self.refresh();
        }
    }

    /// A module installed through `manager`; the toast says so.
    pub fn module_installed(&mut self, manager: &str, now: u64) {
        let said = format!("{manager}: {}", self.state.lang.pick(texts::modules::INSTALL_DONE));
        self.report(&said, now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::testing::*;

    fn row(id: &str, enabled: bool) -> ModuleRow {
        ModuleRow {
            id: id.into(),
            name: format!("Module {id}"),
            note: "v1.0 · someone".into(),
            about: "what it does".into(),
            enabled,
            removing: false,
            pending: false,
            editable: true,
        }
    }

    #[test]
    fn the_page_lists_the_modules_and_opens_one_by_its_place() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Modules, W, H, PPD);
        app.set_modules(Ok(vec![row("a", true), row("b", false)]));
        assert!(tap(&mut app, &f, &i, Action::ModuleOpen(1)).is_empty());
        assert_eq!(app.screen(), Screen::ModuleInfo);
        assert_eq!(app.state.module_at, Some(1));
    }

    #[test]
    fn a_module_whose_next_start_is_already_decided_is_read_only() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Modules, W, H, PPD);
        let locked = ModuleRow { editable: false, ..row("a", true) };
        app.set_modules(Ok(vec![locked]));
        assert!(app.act(Action::ModuleToggle(0), 0).is_empty());
        assert!(app.act(Action::ModuleRemove(0), 0).is_empty());
        assert!(tap(&mut app, &f, &i, Action::ModuleOpen(0)).is_empty());
        assert_eq!(app.screen(), Screen::ModuleInfo);
        let mut c = crate::Canvas::new(W, H);
        app.compose(&mut c, &f, &i, "12:00", 0);
        assert!(app.act(Action::ModuleRemove(0), 0).is_empty());
    }
}
