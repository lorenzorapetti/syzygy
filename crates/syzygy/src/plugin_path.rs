//! Where GStreamer looks for plugins, chosen before anything starts a thread:
//! glib reads the environment back once its threads exist, so this is the
//! only sound place to change it.

/// System GStreamer plugin directories, probed in order, and only when the
/// process is not running from a bundle.
const DIR_CANDIDATES: [&str; 3] = [
    "/usr/lib/x86_64-linux-gnu/gstreamer-1.0",
    "/usr/lib64/gstreamer-1.0",
    "/usr/lib/gstreamer-1.0",
];

/// Set `GST_PLUGIN_PATH` as [`choice`] decides.
pub fn select() {
    let var = |name| std::env::var(name).ok();
    let existing_dirs: Vec<&str> = DIR_CANDIDATES
        .into_iter()
        .filter(|dir| std::path::Path::new(dir).is_dir())
        .collect();
    let chosen = choice(
        var("GST_PLUGIN_SYSTEM_PATH_1_0")
            .or_else(|| var("GST_PLUGIN_SYSTEM_PATH"))
            .as_deref(),
        var("GST_PLUGIN_PATH_1_0").as_deref(),
        var("APPDIR").as_deref(),
        var("GST_PLUGIN_PATH").as_deref(),
        &existing_dirs,
    );
    if let Some(chosen) = chosen {
        // SAFETY: called first thing in `main`, before any thread exists.
        unsafe { std::env::set_var("GST_PLUGIN_PATH", chosen) };
    }
}

/// What `GST_PLUGIN_PATH` should become, or `None` to leave it alone (sone's
/// `gst_plugin_path_choice`). The caller passes only the candidate
/// directories that exist, in preference order.
///
/// 1. Running from a bundle (`GST_PLUGIN_PATH_1_0` or `APPDIR` present): the
///    bundle wins. `GST_PLUGIN_PATH_1_0` **overwrites** an inherited
///    `GST_PLUGIN_PATH`, because a host value leaking into an AppImage points
///    at the host's plugins, which are the wrong ABI.
/// 2. `APPDIR` set but `GST_PLUGIN_PATH_1_0` absent: do nothing at all. A
///    bundle that did not export a plugin path is not asking to be pointed at
///    the host's system directories, so no probing happens.
/// 3. The environment names GStreamer's own system path, as a Nix shell does:
///    it chose its plugins, and the host's would be the wrong ABI. Do nothing.
/// 4. Otherwise, probe the system directories, but only if `GST_PLUGIN_PATH` is
///    not already set.
fn choice(
    system_path: Option<&str>,
    plugin_path_1_0: Option<&str>,
    appdir: Option<&str>,
    plugin_path: Option<&str>,
    existing_dirs: &[&str],
) -> Option<String> {
    if plugin_path_1_0.is_some() || appdir.is_some() {
        return plugin_path_1_0.map(str::to_string);
    }
    if system_path.is_some() || plugin_path.is_some() {
        return None;
    }
    existing_dirs.first().map(|dir| dir.to_string())
}

#[cfg(test)]
mod tests {
    use super::choice;

    const DIRS: [&str; 2] = ["/usr/lib64/gstreamer-1.0", "/usr/lib/gstreamer-1.0"];

    #[test]
    fn bundle_plugin_path_overwrites_an_inherited_one() {
        assert_eq!(
            choice(
                None,
                Some("/app/lib/gstreamer-1.0"),
                None,
                Some("/usr/lib/gstreamer-1.0"),
                &DIRS,
            ),
            Some("/app/lib/gstreamer-1.0".to_string()),
            "a host GST_PLUGIN_PATH leaking into a bundle must not win"
        );
    }

    /// The ordinary AppImage layout: AppRun exports GST_PLUGIN_PATH_1_0 and the
    /// host has no GST_PLUGIN_PATH at all. Every other bundle case here passes a
    /// *set* plugin_path, so without this one an implementation that only
    /// honours _1_0 when something is already set passes the whole suite while
    /// leaving the most common bundle with no plugin path.
    #[test]
    fn a_bundle_plugin_path_is_used_when_nothing_was_inherited() {
        assert_eq!(
            choice(None, Some("/app/lib/gstreamer-1.0"), None, None, &DIRS),
            Some("/app/lib/gstreamer-1.0".to_string()),
            "the canonical AppImage layout must still get the bundle's plugins"
        );
    }

    /// Same, with APPDIR also exported, which is what AppRun actually does.
    #[test]
    fn a_bundle_plugin_path_wins_with_appdir_present_and_nothing_inherited() {
        assert_eq!(
            choice(
                None,
                Some("/app/lib/gstreamer-1.0"),
                Some("/tmp/.mount_syzygy"),
                None,
                &DIRS,
            ),
            Some("/app/lib/gstreamer-1.0".to_string())
        );
    }

    /// Row `(_1_0 set, APPDIR set, GST_PLUGIN_PATH set)`: the bundle still wins.
    #[test]
    fn a_bundle_plugin_path_overwrites_an_inherited_one_with_appdir_present() {
        assert_eq!(
            choice(
                None,
                Some("/app/lib/gstreamer-1.0"),
                Some("/tmp/.mount_syzygy"),
                Some("/usr/lib/gstreamer-1.0"),
                &DIRS,
            ),
            Some("/app/lib/gstreamer-1.0".to_string())
        );
    }

    /// Row `(_1_0 unset, APPDIR set, GST_PLUGIN_PATH set)`: still a bundle, so
    /// still no probe. Without this, a mutant that falls through to the system
    /// directories on this row points an AppImage at host plugins undetected.
    #[test]
    fn appdir_with_an_inherited_path_probes_nothing_either() {
        assert_eq!(
            choice(
                None,
                None,
                Some("/tmp/.mount_syzygy"),
                Some("/usr/lib/gstreamer-1.0"),
                &DIRS,
            ),
            None,
            "a bundle must never be pointed at the host's system plugin dirs"
        );
    }

    #[test]
    fn appdir_without_a_bundle_plugin_path_probes_nothing() {
        assert_eq!(
            choice(None, None, Some("/tmp/.mount_syzygy"), None, &DIRS),
            None,
            "an AppImage must not be pointed at the host's system plugin dirs"
        );
    }

    #[test]
    fn outside_a_bundle_an_unset_path_takes_the_first_existing_dir() {
        assert_eq!(
            choice(None, None, None, None, &DIRS),
            Some("/usr/lib64/gstreamer-1.0".to_string())
        );
    }

    #[test]
    fn outside_a_bundle_an_existing_path_is_left_alone() {
        assert_eq!(choice(None, None, None, Some("/opt/gst"), &DIRS), None);
    }

    #[test]
    fn outside_a_bundle_with_no_existing_dirs_nothing_is_set() {
        assert_eq!(choice(None, None, None, None, &[]), None);
    }

    #[test]
    fn a_chosen_system_path_is_left_to_itself() {
        assert_eq!(
            choice(
                Some("/nix/store/gst-plugins/lib/gstreamer-1.0"),
                None,
                None,
                None,
                &DIRS
            ),
            None,
            "a Nix GStreamer must not load the host's plugins"
        );
    }

    #[test]
    fn a_bundle_still_wins_over_a_chosen_system_path() {
        assert_eq!(
            choice(
                Some("/usr/lib/gstreamer-1.0"),
                Some("/app/lib/gstreamer-1.0"),
                None,
                None,
                &DIRS
            ),
            Some("/app/lib/gstreamer-1.0".to_string())
        );
    }
}
