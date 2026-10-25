use crate::db::{now, Db};
use anyhow::{anyhow, Result};
use rusqlite::params;

/// The traits a player may overrule for one game. Their word beats any tag.
pub const TRAITS: [&str; 3] = ["sound", "pause", "scare"];

/// Pins one trait of one game to a value, or clears the pin with `None`.
pub fn set(db: &Db, appid: i64, name: &str, value: Option<f64>) -> Result<()> {
    if !TRAITS.contains(&name) {
        return Err(anyhow!("unknown trait “{name}”"));
    }
    db.with(|c| {
        match value {
            Some(v) => c.execute(
                "INSERT INTO overrides(appid,trait,value,updated_at) VALUES(?1,?2,?3,?4)
                 ON CONFLICT(appid,trait) DO UPDATE SET value=?3, updated_at=?4",
                params![appid, name, v.clamp(0.0, 1.0), now()],
            )?,
            None => c.execute(
                "DELETE FROM overrides WHERE appid=?1 AND trait=?2",
                params![appid, name],
            )?,
        };
        Ok(())
    })?;
    db.log_note(
        appid,
        "trait_set",
        &format!(
            "{name}={}",
            value
                .map(|v| v.to_string())
                .unwrap_or_else(|| "auto".into())
        ),
    )
}
