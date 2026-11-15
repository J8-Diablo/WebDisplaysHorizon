// WebDisplays is one file: its blocks, items, recipes, textures, sounds and text are the
// content mod in this bundle's data/ (`tools/pack.py` builds content/ into it). It is written
// out once, beside the game's other caches, and loaded as a development mod is, so it is on
// whenever this native mod is built in. An installed webdisplays.hmod that is turned on is
// read first and wins.
//
// This is the only thing the helper does: it adds one path to the list of development mods
// and lets the game's own `content_root` read it, with the checks it makes on every content
// mod. It writes one file, `.horizon-cache/webdisplays-embedded-content.hmod`, in the
// instance's folder, and only when that file is missing or differs.
fn webdisplays_embedded_content(
    root: &std::path::Path,
    enabled: &std::collections::BTreeSet<String>,
    dev: &[std::path::PathBuf],
) -> core::ops::ControlFlow<std::path::PathBuf> {
    static CONTENT: &[u8] = horizon_client::native_data!("webdisplays_native", "webdisplays-content.hmod");
    let dir = std::env::current_dir().unwrap_or_default().join(".horizon-cache");
    let file = dir.join("webdisplays-embedded-content.hmod");
    // The second time through (the call below), the path is in the list: let the game go on.
    if dev.contains(&file) {
        return core::ops::ControlFlow::Continue(());
    }
    let same = std::fs::read(&file).is_ok_and(|on_disk| on_disk == CONTENT);
    if !same && std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&file, CONTENT)).is_err() {
        log::warn!("WebDisplays: its content could not be written to {}", file.display());
        return core::ops::ControlFlow::Continue(());
    }
    let mut with = dev.to_vec();
    with.push(file);
    core::ops::ControlFlow::Break(content_root(root, enabled, &with))
}
