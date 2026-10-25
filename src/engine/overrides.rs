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
        let known: i64 = c.query_row(
            "SELECT COUNT(*) FROM games WHERE appid=?1",
            params![appid],
            |r| r.get(0),
        )?;
        if known == 0 {
            return Err(anyhow!("no such game in your library"));
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::Scratch;

    #[test]
    fn only_games_in_the_library_can_be_overruled() {
        let scratch = Scratch::new();
        assert!(set(&scratch.db, 999, "sound", Some(0.5)).is_err());
        scratch
            .db
            .with(|c| {
                c.execute("INSERT INTO games(appid,name) VALUES(5,'Five')", [])?;
                Ok(())
            })
            .unwrap();
        assert!(set(&scratch.db, 5, "sound", Some(0.5)).is_ok());
        assert!(set(&scratch.db, 5, "nonsense", Some(0.5)).is_err());
        assert!(set(&scratch.db, 5, "sound", None).is_ok());
    }
}
