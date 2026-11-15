//! Business logic, free of egui so it can be unit-tested without a GUI.

pub mod auth;
pub mod config;
pub mod error;
pub mod export;
pub mod matching;
pub mod spotify;
pub mod youtube;

/// Shown verbatim in the README and on the About screen.
pub const DISCLAIMER: &str = "playlist-porter is an independent project. It is not affiliated with, endorsed by, or sponsored by Spotify or Google/YouTube. Track metadata is supplied by Spotify. This tool exports only playlist metadata from your own account through official APIs and never downloads audio. You are solely responsible for how you use the exported data and for following Spotify's and YouTube's terms and copyright law where you live. The authors are not liable for any misuse, including downloading or sharing copyrighted material.";

#[cfg(test)]
mod tests {
    #[test]
    fn disclaimer_is_verbatim_from_claude_md() {
        let md = include_str!("../../CLAUDE.md");
        let start = md
            .find("> playlist-porter is an independent project.")
            .unwrap();
        let block: Vec<&str> = md[start..]
            .lines()
            .take_while(|l| l.starts_with('>'))
            .map(|l| l.trim_start_matches('>').trim())
            .collect();
        assert_eq!(block.join(" "), super::DISCLAIMER);
    }
}
