use crate::db::{DbError, Result};
use rusqlite::{Connection, OptionalExtension, params};

#[derive(Debug, Clone, PartialEq)]
pub struct VoiceSample {
    pub embedding: Vec<f32>,
    pub model: String,
    pub created: i64,
}

pub fn save(c: &Connection, s: &VoiceSample) -> Result<()> {
    let embedding: Vec<u8> = s.embedding.iter().flat_map(|v| v.to_le_bytes()).collect();
    c.execute(
        "INSERT INTO voice_sample(id,embedding,model,created) VALUES(1,?1,?2,?3)
         ON CONFLICT(id) DO UPDATE SET embedding=excluded.embedding,model=excluded.model,created=excluded.created",
        params![embedding, s.model, s.created],
    )?;
    Ok(())
}

pub fn load(c: &Connection) -> Result<Option<VoiceSample>> {
    let stored: Option<(Vec<u8>, String, i64)> = c
        .query_row(
            "SELECT embedding,model,created FROM voice_sample WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    stored
        .map(|(bytes, model, created)| {
            if bytes.len() % 4 != 0 {
                return Err(DbError::Storage("invalid voice sample embedding"));
            }
            let embedding = bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| f32::from_le_bytes(*b))
                .collect();
            Ok(VoiceSample {
                embedding,
                model,
                created,
            })
        })
        .transpose()
}

pub fn delete(c: &Connection) -> Result<bool> {
    Ok(c.execute("DELETE FROM voice_sample WHERE id=1", [])? > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    #[test]
    fn sample_round_trips_replaces_and_deletes() {
        let db = Database::in_memory().unwrap();
        let c = db.connection();
        assert_eq!(load(c).unwrap(), None);
        assert!(!delete(c).unwrap());

        let mut sample = VoiceSample {
            embedding: vec![1.0, -0.5],
            model: "first.onnx".into(),
            created: 10,
        };
        save(c, &sample).unwrap();
        let bytes: Vec<u8> = c
            .query_row("SELECT embedding FROM voice_sample", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            bytes,
            [1.0_f32.to_le_bytes(), (-0.5_f32).to_le_bytes()].concat()
        );
        assert_eq!(load(c).unwrap(), Some(sample.clone()));

        sample.embedding = vec![0.25];
        sample.model = "second.onnx".into();
        sample.created = 20;
        save(c, &sample).unwrap();
        assert_eq!(load(c).unwrap(), Some(sample));
        let rows: i64 = c
            .query_row("SELECT count(*) FROM voice_sample", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1);
        assert!(delete(c).unwrap());
        assert_eq!(load(c).unwrap(), None);
        assert!(!delete(c).unwrap());
    }

    #[test]
    fn malformed_embedding_is_rejected() {
        let db = Database::in_memory().unwrap();
        let c = db.connection();
        c.execute(
            "INSERT INTO voice_sample(id,embedding,model,created) VALUES(1,?1,'model',1)",
            [vec![1_u8, 2, 3]],
        )
        .unwrap();
        assert!(load(c).is_err());
    }
}
