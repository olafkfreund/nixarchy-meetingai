//! Meeting export: renders `notes.md` and writes the meeting folder (plain files, no database).

use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};

const MAX_NAME: usize = 120;

pub struct Item {
    /// "action", "decision" or "question".
    pub kind: String,
    pub text: String,
    pub owner: Option<String>,
    pub due: Option<String>,
}

pub struct Line {
    pub at: f64,
    pub speaker: String,
    pub text: String,
}

pub struct Shot {
    pub taken_at: f64,
    pub question: String,
    pub mime: String,
    pub data: Vec<u8>,
}

pub struct Doc {
    pub name: String,
    pub data: Vec<u8>,
}

pub struct Export {
    pub meeting_id: String,
    pub title: String,
    /// `YYYY-MM-DD`, the meeting's start date; names the folder.
    pub date: String,
    pub start: String,
    pub end: String,
    pub duration: String,
    pub mode: String,
    pub candidate: Option<String>,
    pub local_only: bool,
    pub exported: String,
    /// The recap, else the rolling notes.
    pub summary: String,
    pub items: Vec<Item>,
    pub lines: Vec<Line>,
    pub shots: Vec<Shot>,
    pub docs: Vec<Doc>,
}

fn clock(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

fn shot_name(i: usize, mime: &str) -> String {
    let ext = if mime.contains("jpeg") { "jpg" } else { "png" };
    format!("shot-{:02}.{ext}", i + 1)
}

/// Safe, unique names: a repeated name gets " (2)", " (3)" … before its extension.
fn doc_names(docs: &[Doc]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for d in docs {
        let name = safe_name(&d.name);
        let (stem, ext) = match name.rfind('.') {
            Some(i) if i > 0 => (&name[..i], &name[i..]),
            _ => (name.as_str(), ""),
        };
        let mut n = name.clone();
        for k in 2.. {
            if !out.contains(&n) {
                break;
            }
            n = format!("{stem} ({k}){ext}");
        }
        out.push(n);
    }
    out
}

fn cut(s: &str, max: usize) -> &str {
    let mut end = max.min(s.len());
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// A string that is safe as one file or folder name on any platform.
pub fn safe_name(s: &str) -> String {
    let swapped: String = s
        .chars()
        .map(|c| {
            if c.is_control() || "/\\<>:\"|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let mut name = swapped.trim_start_matches('.').trim().to_string();
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.ends_with(|c: char| c.is_ascii_digit() && c != '0'));
    if reserved {
        name.push('_');
    }
    let name = cut(&name, MAX_NAME).trim_end_matches(['.', ' ']);
    if name.is_empty() {
        "Meeting".into()
    } else {
        name.into()
    }
}

fn q(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_default()
}

pub fn render(e: &Export) -> String {
    let mut o = format!(
        "---\ntitle: {}\ndate: {}\nstart: {}\nend: {}\nduration: {}\nmode: {}\n",
        q(&e.title),
        e.date,
        q(&e.start),
        q(&e.end),
        q(&e.duration),
        e.mode
    );
    if let Some(c) = &e.candidate {
        o += &format!("candidate: {}\nlocal_only: {}\n", q(c), e.local_only);
    }
    o += &format!(
        "meeting_id: {}\nexported: {}\n---\n\n# {}\n",
        q(&e.meeting_id),
        q(&e.exported),
        e.title
    );
    if !e.summary.trim().is_empty() {
        o += &format!("\n## Summary\n\n{}\n", e.summary.trim());
    }
    for (kind, head) in [
        ("action", "Action items"),
        ("decision", "Decisions"),
        ("question", "Open questions"),
    ] {
        let items: Vec<_> = e.items.iter().filter(|i| i.kind == kind).collect();
        if items.is_empty() {
            continue;
        }
        o += &format!("\n## {head}\n\n");
        for i in items {
            o += &format!("- {}", i.text);
            if let Some(w) = &i.owner {
                o += &format!(" (owner: {w})");
            }
            if let Some(d) = &i.due {
                o += &format!(" (due: {d})");
            }
            o += "\n";
        }
    }
    if !e.shots.is_empty() {
        o += "\n## Screenshots\n\n";
        for (i, s) in e.shots.iter().enumerate() {
            let t = clock(s.taken_at);
            o += &format!(
                "![{t}](attachments/{})\n\n{t} {}\n\n",
                shot_name(i, &s.mime),
                s.question.trim()
            );
        }
    }
    if !e.docs.is_empty() {
        o += "\n## Documents\n\n";
        for n in doc_names(&e.docs) {
            o += &format!("- [{n}](documents/{})\n", n.replace(' ', "%20"));
        }
    }
    if !e.lines.is_empty() {
        o += "\n## Transcript\n\n";
        for l in &e.lines {
            o += &format!("[{}] {}: {}\n", clock(l.at), l.speaker, l.text);
        }
    }
    o
}

/// Write `rel` under `dir` with mode 0600; refuses anything that is not a plain relative path.
fn put(dir: &Path, rel: &Path, data: &[u8]) -> io::Result<()> {
    if !rel.components().all(|c| matches!(c, Component::Normal(_))) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "path outside the meeting folder",
        ));
    }
    let mut f = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(dir.join(rel))?;
    f.write_all(data)
}

fn mkdir(p: &Path) -> io::Result<()> {
    DirBuilder::new().recursive(true).mode(0o700).create(p)
}

/// Replace `folder/name` with a directory holding `files`, built in a temporary sibling first.
/// With no files the directory is removed: the app owns it.
fn swap_dir(folder: &Path, name: &str, files: &[(String, &[u8])]) -> io::Result<()> {
    let dest = folder.join(name);
    if files.is_empty() {
        return match fs::remove_dir_all(&dest) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    let tmp = folder.join(format!(".{name}.tmp"));
    let _ = fs::remove_dir_all(&tmp);
    mkdir(&tmp)?;
    for (n, d) in files {
        put(&tmp, Path::new(n), d)?;
    }
    if dest.exists() {
        fs::remove_dir_all(&dest)?;
    }
    fs::rename(tmp, dest)
}

/// Write the meeting under `root`, reusing (and renaming if needed) `previous`. Returns its folder.
pub fn write(root: &Path, previous: Option<&Path>, e: &Export) -> io::Result<PathBuf> {
    mkdir(root)?;
    let base = cut(&format!("{} {}", e.date, safe_name(&e.title)), MAX_NAME)
        .trim_end()
        .to_string();
    let prev = previous.filter(|p| p.is_dir() && p.parent() == Some(root));
    let current = prev.and_then(|p| p.file_name()?.to_str());
    let keeps = |n: &str| {
        n == base
            || n.strip_prefix(&format!("{base} ("))
                .is_some_and(|r| r.ends_with(')'))
    };
    let folder = match (prev, current) {
        (Some(p), Some(n)) if keeps(n) => p.to_path_buf(),
        _ => {
            let mut dest = root.join(&base);
            for n in 2.. {
                if !dest.exists() {
                    break;
                }
                dest = root.join(format!("{base} ({n})"));
            }
            match prev {
                Some(p) => fs::rename(p, &dest)?,
                None => mkdir(&dest)?,
            }
            dest
        }
    };
    let tmp = ".notes.md.tmp";
    put(&folder, Path::new(tmp), render(e).as_bytes())?;
    fs::rename(folder.join(tmp), folder.join("notes.md"))?;
    let shots: Vec<_> = e
        .shots
        .iter()
        .enumerate()
        .map(|(i, s)| (shot_name(i, &s.mime), s.data.as_slice()))
        .collect();
    let docs: Vec<_> = doc_names(&e.docs)
        .into_iter()
        .zip(e.docs.iter().map(|d| d.data.as_slice()))
        .collect();
    swap_dir(&folder, "attachments", &shots)?;
    swap_dir(&folder, "documents", &docs)?;
    Ok(folder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    struct Tmp(PathBuf);
    impl Tmp {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!("export-test-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&p).unwrap();
            Tmp(p)
        }
    }
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn fixture(title: &str) -> Export {
        Export {
            meeting_id: "m-7".into(),
            title: title.into(),
            date: "2026-10-30".into(),
            start: "2026-10-30T09:00:00".into(),
            end: "2026-10-30T09:30:00".into(),
            duration: "00:30:00".into(),
            mode: "interviewer".into(),
            candidate: Some("Ada".into()),
            local_only: true,
            exported: "2026-10-30T10:00:00".into(),
            summary: "We shipped it.".into(),
            items: vec![
                Item {
                    kind: "action".into(),
                    text: "Send CV".into(),
                    owner: Some("Bo".into()),
                    due: Some("Friday".into()),
                },
                Item {
                    kind: "decision".into(),
                    text: "Go".into(),
                    owner: None,
                    due: None,
                },
            ],
            lines: vec![Line {
                at: 3725.0,
                speaker: "Ada".into(),
                text: "Hello".into(),
            }],
            shots: vec![Shot {
                taken_at: 61.0,
                question: "What is this?".into(),
                mime: "image/png".into(),
                data: vec![1, 2, 3],
            }],
            docs: vec![Doc {
                name: "cv.pdf".into(),
                data: b"%PDF".to_vec(),
            }],
        }
    }

    #[test]
    fn render_fixture() {
        let s = render(&fixture("Sync"));
        assert!(s.starts_with("---\ntitle: \"Sync\"\ndate: 2026-10-30\n"));
        assert!(s.contains("candidate: \"Ada\"\nlocal_only: true\nmeeting_id: \"m-7\"\n"));
        assert!(s.contains("# Sync\n\n## Summary\n\nWe shipped it.\n"));
        assert!(s.contains("## Action items\n\n- Send CV (owner: Bo) (due: Friday)\n"));
        assert!(s.contains("## Decisions\n\n- Go\n"));
        assert!(!s.contains("## Open questions"));
        assert!(s.contains("![00:01:01](attachments/shot-01.png)"));
        assert!(s.contains("- [cv.pdf](documents/cv.pdf)"));
        assert!(s.contains("[01:02:05] Ada: Hello\n"));
        assert!(!s.contains("![["));
    }

    #[test]
    fn safe_name_cases() {
        assert_eq!(safe_name("a/b\\c"), "a_b_c");
        assert_eq!(safe_name(".."), "Meeting");
        assert_eq!(safe_name("../etc"), "_etc");
        assert_eq!(safe_name("a\u{7}b"), "a_b");
        assert_eq!(safe_name("a<>:\"|?*b"), "a_______b");
        assert_eq!(safe_name("CON"), "CON_");
        assert_eq!(safe_name("nul.txt"), "nul.txt_");
        assert_eq!(safe_name("com1"), "com1_");
        assert_eq!(safe_name("com0"), "com0");
        assert_eq!(safe_name(".hidden"), "hidden");
        assert_eq!(safe_name("end. . "), "end");
        assert_eq!(safe_name(""), "Meeting");
        assert_eq!(safe_name("   "), "Meeting");
        let long = safe_name(&"é".repeat(100));
        assert!(long.len() <= MAX_NAME && long.chars().all(|c| c == 'é'));
    }

    #[test]
    fn put_refuses_outside() {
        let t = Tmp::new();
        assert!(put(&t.0, Path::new("../x"), b"x").is_err());
        assert!(put(&t.0, Path::new("/tmp/x"), b"x").is_err());
        assert!(put(&t.0, Path::new("ok"), b"x").is_ok());
    }

    #[test]
    fn rename_keeps_user_file_and_collision_suffix() {
        let t = Tmp::new();
        let root = t.0.join("out");
        let a = write(&root, None, &fixture("Old")).unwrap();
        assert_eq!(a, root.join("2026-10-30 Old"));
        fs::write(a.join("mine.txt"), "keep").unwrap();
        let b = write(&root, Some(&a), &fixture("New")).unwrap();
        assert_eq!(b, root.join("2026-10-30 New"));
        assert!(!a.exists());
        assert_eq!(fs::read_to_string(b.join("mine.txt")).unwrap(), "keep");
        assert!(
            b.join("notes.md").is_file()
                && b.join("attachments/shot-01.png").is_file()
                && b.join("documents/cv.pdf").is_file()
        );
        // same name, no previous: another folder is taken, so (2); re-export keeps it
        let c = write(&root, None, &fixture("New")).unwrap();
        assert_eq!(c, root.join("2026-10-30 New (2)"));
        assert_eq!(write(&root, Some(&c), &fixture("New")).unwrap(), c);
        assert_eq!(write(&root, Some(&b), &fixture("New")).unwrap(), b);
    }

    #[test]
    fn modes_and_no_wiki_embeds() {
        let t = Tmp::new();
        let root = t.0.join("out");
        let f = write(&root, None, &fixture("Modes")).unwrap();
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        for d in [
            root.clone(),
            f.clone(),
            f.join("attachments"),
            f.join("documents"),
        ] {
            assert_eq!(mode(&d), 0o700, "{d:?}");
        }
        for p in [
            f.join("notes.md"),
            f.join("attachments/shot-01.png"),
            f.join("documents/cv.pdf"),
        ] {
            assert_eq!(mode(&p), 0o600, "{p:?}");
        }
        assert!(
            !fs::read_to_string(f.join("notes.md"))
                .unwrap()
                .contains("![[")
        );
    }

    #[test]
    fn duplicate_doc_names_and_stale_dirs() {
        let docs = ["cv.pdf", "cv.pdf", "cv.pdf", "notes"].map(|n| Doc {
            name: n.into(),
            data: vec![],
        });
        assert_eq!(
            doc_names(&docs),
            ["cv.pdf", "cv (2).pdf", "cv (3).pdf", "notes"]
        );
        let t = Tmp::new();
        let first = write(&t.0, None, &fixture("Sync")).unwrap();
        assert!(first.join("attachments").is_dir() && first.join("documents").is_dir());
        let mut bare = fixture("Sync");
        bare.shots.clear();
        bare.docs.clear();
        let again = write(&t.0, Some(&first), &bare).unwrap();
        assert_eq!(again, first);
        assert!(!again.join("attachments").exists() && !again.join("documents").exists());
    }
}
