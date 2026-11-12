use crate::state::{App, Op};

impl App {
    /// The profiles on the mouse with the queued NEW and DELETE already applied.
    /// A mouse that doesn't answer the profile commands gets its device file's list, minus P0.
    pub(crate) fn profile_list(&self) -> Vec<u8> {
        let mut list = match (&self.onboard, &self.profile_presets) {
            (Some(on), _) => on.ids.clone(),
            (None, Some(presets)) => presets.ids.clone(),
            (None, None) => {
                let all = self.mice.get(self.current).map_or(vec![0], |m| m.def.profiles.clone());
                return all.iter().copied().filter(|&p| p != 0 || all.len() == 1).collect();
            }
        };
        for op in &self.ops {
            match op {
                Op::CreateProfile(p) if !list.contains(p) => list.push(*p),
                Op::DeleteProfile(p) => list.retain(|x| x != p),
                _ => {}
            }
        }
        list.sort();
        list
    }

    pub(crate) fn profile_name(&self, profile: u8) -> Option<String> {
        // a queued DELETE PROFILE forgets the saved name, so a NEW PROFILE with the same number starts without one
        let mut name = self.names.get(&profile).cloned();
        for op in &self.ops {
            match op {
                Op::DeleteProfile(p) if *p == profile => name = None,
                Op::Rename { profile: p, name: n } if *p == profile => name = Some(n.clone()),
                _ => {}
            }
        }
        name.filter(|n| !n.trim().is_empty())
    }

    pub(crate) fn active_profile(&self) -> Option<u8> {
        self.onboard.as_ref().map(|o| o.active).or(self.profile_presets.as_ref().map(|p| p.active))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lib::profile_presets::Presets;
    use eframe::egui;
    use opensynapse::device::Profiles;
    use std::collections::BTreeMap;

    fn app_with(profiles: &[u8]) -> App {
        let mut app = App::blank(egui::Context::default());
        let mut mouse = opensynapse::device::offline_mouse();
        mouse.def.profiles = profiles.to_vec();
        app.mice.push(mouse);
        app
    }

    fn onboard(ids: &[u8], active: u8) -> Option<Profiles> {
        Some(Profiles { max: 5, ids: ids.to_vec(), active })
    }

    fn rename(profile: u8, name: &str) -> Op {
        Op::Rename { profile, name: name.into() }
    }

    #[test]
    fn profile_list_applies_queued_creates_and_deletes_to_the_mouse() {
        let mut app = app_with(&[0, 1, 2]);
        app.onboard = onboard(&[1, 3], 1);
        assert_eq!(app.profile_list(), [1, 3]);
        app.ops = vec![Op::CreateProfile(2), Op::DeleteProfile(1)];
        assert_eq!(app.profile_list(), [2, 3]);
        // creating one that exists doesn't repeat it, and the list stays sorted
        app.ops = vec![Op::CreateProfile(3), Op::CreateProfile(5), Op::CreateProfile(2)];
        assert_eq!(app.profile_list(), [1, 2, 3, 5]);
        // later ops win, so a delete then a create brings it back
        app.ops = vec![Op::DeleteProfile(1), Op::CreateProfile(1)];
        assert_eq!(app.profile_list(), [1, 3]);
        // ops that aren't about profiles change nothing
        app.ops = vec![Op::Poll(500), Op::SetActive(3), rename(1, "x")];
        assert_eq!(app.profile_list(), [1, 3]);
    }

    #[test]
    fn profile_list_uses_the_presets_when_the_mouse_has_no_profiles() {
        let mut app = app_with(&[0, 1]);
        app.profile_presets = Some(Presets { ids: vec![1, 2], active: 1 });
        app.ops = vec![Op::CreateProfile(4), Op::DeleteProfile(2)];
        assert_eq!(app.profile_list(), [1, 4]);
        // what the mouse reports wins when it answers
        app.onboard = onboard(&[1], 1);
        assert_eq!(app.profile_list(), [1, 4]);
        app.ops.clear();
        assert_eq!(app.profile_list(), [1]);
    }

    #[test]
    fn profile_list_falls_back_to_the_device_file_without_p0() {
        let mut app = app_with(&[0, 1, 2]);
        assert_eq!(app.profile_list(), [1, 2]);
        // queued ops only apply to a list we know, the fallback stays as the file has it
        app.ops = vec![Op::CreateProfile(7)];
        assert_eq!(app.profile_list(), [1, 2]);
        // P0 stays when it's the only profile
        assert_eq!(app_with(&[0]).profile_list(), [0]);
        // no mouse at all
        assert_eq!(App::blank(egui::Context::default()).profile_list(), [0]);
    }

    #[test]
    fn profile_name_follows_the_queue() {
        let mut app = app_with(&[0, 1]);
        app.names = BTreeMap::from([(1, "Work".into()), (2, "  ".into())]);
        assert_eq!(app.profile_name(1).as_deref(), Some("Work"));
        assert_eq!(app.profile_name(3), None);
        // a name that's only spaces is no name
        assert_eq!(app.profile_name(2), None);

        app.ops = vec![rename(1, "Game")];
        assert_eq!(app.profile_name(1).as_deref(), Some("Game"));
        app.ops = vec![rename(1, "")];
        assert_eq!(app.profile_name(1), None);
        app.ops = vec![rename(3, "New")];
        assert_eq!(app.profile_name(3).as_deref(), Some("New"));
        // a rename for another profile doesn't touch this one
        assert_eq!(app.profile_name(1).as_deref(), Some("Work"));

        // DELETE forgets the saved name, so a NEW PROFILE with that number starts blank
        app.ops = vec![Op::DeleteProfile(1)];
        assert_eq!(app.profile_name(1), None);
        app.ops = vec![Op::DeleteProfile(1), Op::CreateProfile(1)];
        assert_eq!(app.profile_name(1), None);
        // a rename queued after the delete applies again, one before it is dropped
        app.ops = vec![Op::DeleteProfile(1), rename(1, "Fresh")];
        assert_eq!(app.profile_name(1).as_deref(), Some("Fresh"));
        app.ops = vec![rename(1, "Old"), Op::DeleteProfile(1)];
        assert_eq!(app.profile_name(1), None);
    }

    #[test]
    fn active_profile_is_the_mouses_else_the_presets() {
        let mut app = app_with(&[0, 1]);
        assert_eq!(app.active_profile(), None);
        app.profile_presets = Some(Presets { ids: vec![1, 2], active: 2 });
        assert_eq!(app.active_profile(), Some(2));
        app.onboard = onboard(&[1, 3], 3);
        assert_eq!(app.active_profile(), Some(3));
    }
}
