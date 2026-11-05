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
    /// "open", "done" or "dropped"; dropped items are left out.
    pub status: String,
}

pub struct Line {
    pub at: f64,
    pub speaker: String,
    pub text: String,
}

pub struct Shot {
    pub automatic: bool,
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

fn shot_name(i: usize, mime: &str, automatic: bool) -> String {
    let ext = if mime.contains("jpeg") { "jpg" } else { "png" };
    format!(
        "{}-{:02}.{ext}",
        if automatic { "auto" } else { "shot" },
        i + 1
    )
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

/// Percent-encode every byte but ASCII letters, digits and `-._~`.
fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
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
    let names: Vec<_> = e
        .shots
        .iter()
        .scan((0, 0), |(manual, auto), s| {
            let i = if s.automatic { auto } else { manual };
            let name = shot_name(*i, &s.mime, s.automatic);
            *i += 1;
            Some(name)
        })
        .collect();
    render_with_shots(e, &names)
}

fn render_with_shots(e: &Export, names: &[String]) -> String {
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
    // The recap usually opens with its own "## Summary" heading; keep one.
    let summary = e.summary.trim();
    let summary = summary
        .strip_prefix("## Summary")
        .map_or(summary, str::trim_start);
    if !summary.is_empty() {
        o += &format!("\n## Summary\n\n{summary}\n");
    }
    for (kind, head) in [
        ("action", "Action items"),
        ("decision", "Decisions"),
        ("question", "Open questions"),
    ] {
        let items: Vec<_> = e
            .items
            .iter()
            .filter(|i| i.kind == kind && i.status != "dropped")
            .collect();
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
            if i.status == "done" {
                o += " (done)";
            }
            o += "\n";
        }
    }
    if !e.shots.is_empty() {
        o += "\n## Screenshots\n\n";
        for (s, name) in e.shots.iter().zip(names) {
            let t = clock(s.taken_at);
            o += &format!(
                "![{t}](screenshots/{})\n\n{t} {}\n\n",
                encode(name),
                s.question.trim()
            );
        }
    }
    if !e.docs.is_empty() {
        o += "\n## Documents\n\n";
        for n in doc_names(&e.docs) {
            let label = n
                .replace('\\', "\\\\")
                .replace('[', "\\[")
                .replace(']', "\\]");
            o += &format!("- [{label}](documents/{})\n", encode(&n));
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
/// `create_new` (O_EXCL) never follows a symlink, so a planted one makes this fail.
fn put(dir: &Path, rel: &Path, data: &[u8]) -> io::Result<()> {
    if !rel.components().all(|c| matches!(c, Component::Normal(_))) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "path outside the meeting folder",
        ));
    }
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(dir.join(rel))?;
    f.write_all(data)
}

fn mkdir(p: &Path) -> io::Result<()> {
    DirBuilder::new().recursive(true).mode(0o700).create(p)
}

/// Remove the entry itself (a symlink is unlinked, never followed); a missing one is fine.
fn rm(p: &Path) -> io::Result<()> {
    let r = match fs::symlink_metadata(p) {
        Ok(m) if m.is_dir() => fs::remove_dir_all(p),
        Ok(_) => fs::remove_file(p),
        Err(e) => Err(e),
    };
    match r {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

fn real_dir(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok_and(|m| m.is_dir())
}

/// Does `folder/notes.md` name this meeting in its frontmatter? `None` when there is no readable one.
fn names(folder: &Path, id: &str) -> Option<bool> {
    let text = fs::read_to_string(folder.join("notes.md")).ok()?;
    let want = format!("meeting_id: {}", q(id));
    let mut lines = text.lines();
    Some(lines.next() == Some("---") && lines.take_while(|l| *l != "---").any(|l| l == want))
}

fn sha256(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Write `files` into `folder/name`, keeping any other files already there; remove the directory
/// only when `files` is empty and nothing else is in it.
fn write_dir(folder: &Path, name: &str, files: &[(String, &[u8])]) -> io::Result<()> {
    let dest = folder.join(name);
    if files.is_empty() {
        let _ = fs::remove_dir(&dest);
        return Ok(());
    }
    if fs::symlink_metadata(&dest).is_ok_and(|m| !m.is_dir()) {
        fs::remove_file(&dest)?;
    }
    mkdir(&dest)?;
    for (n, d) in files {
        let tmp = format!(".{n}.tmp");
        rm(&dest.join(&tmp))?;
        put(&dest, Path::new(&tmp), d)?;
        if let Err(err) = fs::rename(dest.join(&tmp), dest.join(n)) {
            let _ = rm(&dest.join(&tmp));
            return Err(err);
        }
    }
    Ok(())
}

/// Refresh only exporter-owned screenshot files, preserving user changes and other entries.
pub fn sync_shots(folder: &Path, shots: &[Shot]) -> io::Result<Vec<String>> {
    let dir = folder.join("screenshots");
    if shots.is_empty() && !dir.exists() {
        return Ok(Vec::new());
    }
    match fs::symlink_metadata(&dir) {
        Ok(meta) if meta.file_type().is_symlink() => fs::remove_file(&dir)?,
        Ok(meta) if !meta.is_dir() => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "screenshots is not a directory",
            ));
        }
        _ => {}
    }
    mkdir(&dir)?;
    let manifest = dir.join(".owned-files");
    let previous = fs::read_to_string(&manifest).unwrap_or_default();
    let owned: Vec<_> = previous
        .lines()
        .filter_map(|line| {
            let (hash, name) = line.split_once("  ")?;
            (hash.len() == 64 && !name.contains(['/', '\\']) && !name.is_empty())
                .then_some((hash, name))
        })
        .collect();
    let (mut manual, mut auto) = (0, 0);
    let mut current = Vec::new();
    for shot in shots {
        let i = if shot.automatic {
            &mut auto
        } else {
            &mut manual
        };
        let base = shot_name(*i, &shot.mime, shot.automatic);
        *i += 1;
        let hash = sha256(&shot.data);
        let (stem, ext) = base.rsplit_once('.').unwrap();
        let mut chosen = None;
        for suffix in 0.. {
            let name = if suffix == 0 {
                base.clone()
            } else {
                format!("{stem}-{suffix}.{ext}")
            };
            if current.iter().any(|(n, _): &(String, String)| n == &name) {
                continue;
            }
            let path = dir.join(&name);
            let ours = match fs::symlink_metadata(&path) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => true,
                Err(_) => false,
                Ok(meta) if meta.is_file() => fs::read(&path).is_ok_and(|bytes| {
                    bytes == shot.data
                        || owned.iter().any(|(old_hash, old_name)| {
                            *old_name == name && *old_hash == sha256(&bytes)
                        })
                }),
                Ok(_) => false,
            };
            if ours {
                chosen = Some(name);
                break;
            }
        }
        let name = chosen.expect("a free screenshot name exists");
        let tmp = format!(".{name}.tmp");
        rm(&dir.join(&tmp))?;
        put(&dir, Path::new(&tmp), &shot.data)?;
        if let Err(error) = fs::rename(dir.join(&tmp), dir.join(&name)) {
            let _ = rm(&dir.join(&tmp));
            return Err(error);
        }
        current.push((name, hash));
    }
    for (hash, name) in owned {
        if current.iter().any(|(n, _)| n == name) {
            continue;
        }
        let path = dir.join(name);
        if fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_file())
            && fs::read(&path).is_ok_and(|bytes| sha256(&bytes) == hash)
        {
            fs::remove_file(path)?;
        }
    }
    let text = current
        .iter()
        .map(|(name, hash)| format!("{hash}  {name}\n"))
        .collect::<String>();
    rm(&dir.join(".owned-files.tmp"))?;
    put(&dir, Path::new(".owned-files.tmp"), text.as_bytes())?;
    fs::rename(dir.join(".owned-files.tmp"), manifest)?;
    Ok(current.into_iter().map(|(name, _)| name).collect())
}

/// Choose, rename or create the meeting folder under `root`. Returns it; writes no contents.
pub fn place(root: &Path, previous: Option<&Path>, e: &Export) -> io::Result<PathBuf> {
    mkdir(root)?;
    let base = cut(&format!("{} {}", e.date, safe_name(&e.title)), MAX_NAME)
        .trim_end()
        .to_string();
    // Someone else's folder (another meeting, a symlink, elsewhere) is never reused.
    let prev = previous.filter(|p| {
        p.parent() == Some(root) && real_dir(p) && names(p, &e.meeting_id) != Some(false)
    });
    let current = prev.and_then(|p| p.file_name()?.to_str());
    let keeps = |n: &str| {
        n == base
            || n.strip_prefix(&format!("{base} ("))
                .is_some_and(|r| r.ends_with(')'))
    };
    if let (Some(p), Some(n)) = (prev, current)
        && keeps(n)
    {
        return Ok(p.to_path_buf());
    }
    let mut dest = root.join(&base);
    for n in 2.. {
        let free = fs::symlink_metadata(&dest).is_err();
        if free || (prev.is_none() && real_dir(&dest) && names(&dest, &e.meeting_id) == Some(true))
        {
            break;
        }
        dest = root.join(format!("{base} ({n})"));
    }
    match prev {
        Some(p) => fs::rename(p, &dest)?,
        None => mkdir(&dest)?,
    }
    Ok(dest)
}

/// Write the meeting's files into `folder`, keeping an edited `notes.md` beside `notes (updated).md`.
/// Returns the sha256 of the notes file written.
pub fn fill_tracked(folder: &Path, e: &Export, stored: Option<&str>) -> io::Result<String> {
    for n in [
        ".notes.md.tmp",
        ".screenshots.tmp",
        ".screenshots.old",
        ".documents.tmp",
        ".documents.old",
    ] {
        rm(&folder.join(n))?; // leftovers of a crash
    }
    let docs: Vec<_> = doc_names(&e.docs)
        .into_iter()
        .zip(e.docs.iter().map(|d| d.data.as_slice()))
        .collect();
    let shot_names = sync_shots(folder, &e.shots)?;
    write_dir(folder, "documents", &docs)?;
    // The first name that is free or still holds exactly what we last wrote; never one the user changed.
    let ours = |name: &String| match fs::symlink_metadata(folder.join(name)) {
        Err(_) => true,
        Ok(_) => stored.is_some_and(|s| {
            fs::read(folder.join(name))
                .ok()
                .is_some_and(|b| sha256(&b) == s)
        }),
    };
    let target = std::iter::once("notes.md".to_owned())
        .chain(std::iter::once("notes (updated).md".to_owned()))
        .chain((2..).map(|n| format!("notes (updated {n}).md")))
        .find(ours)
        .expect("an unbounded list of names");
    let rendered = render_with_shots(e, &shot_names);
    let tmp = ".notes.md.tmp";
    put(folder, Path::new(tmp), rendered.as_bytes())?;
    if let Err(err) = fs::rename(folder.join(tmp), folder.join(&target)) {
        let _ = rm(&folder.join(tmp));
        return Err(err);
    }
    Ok(sha256(rendered.as_bytes()))
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

    fn write(root: &Path, previous: Option<&Path>, e: &Export) -> io::Result<PathBuf> {
        let f = place(root, previous, e)?;
        fill_tracked(&f, e, None)?;
        Ok(f)
    }

    fn sha(data: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        Sha256::digest(data)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
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
                    status: "done".into(),
                },
                Item {
                    kind: "decision".into(),
                    text: "Go".into(),
                    owner: None,
                    due: None,
                    status: "open".into(),
                },
                Item {
                    kind: "decision".into(),
                    text: "Stop".into(),
                    owner: None,
                    due: None,
                    status: "dropped".into(),
                },
            ],
            lines: vec![Line {
                at: 3725.0,
                speaker: "Ada".into(),
                text: "Hello".into(),
            }],
            shots: vec![Shot {
                automatic: false,
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
        let mut headed = fixture("Sync");
        headed.summary = "## Summary\n\nWe shipped it.".into();
        let h = render(&headed);
        assert_eq!(h.matches("## Summary").count(), 1);
        assert!(h.contains("## Summary\n\nWe shipped it.\n"));
        assert!(s.contains("## Action items\n\n- Send CV (owner: Bo) (due: Friday) (done)\n"));
        assert!(s.contains("## Decisions\n\n- Go\n"));
        assert!(!s.contains("## Open questions") && !s.contains("Stop"));
        assert!(s.contains("![00:01:01](screenshots/shot-01.png)"));
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
                && b.join("screenshots/shot-01.png").is_file()
                && b.join("documents/cv.pdf").is_file()
        );
        // our own folder is reused even without a record; another meeting's gets (2)
        assert_eq!(write(&root, None, &fixture("New")).unwrap(), b);
        let mut other = fixture("New");
        other.meeting_id = "m-8".into();
        let c = write(&root, None, &other).unwrap();
        assert_eq!(c, root.join("2026-10-30 New (2)"));
        assert_eq!(write(&root, Some(&c), &other).unwrap(), c);
        assert_eq!(write(&root, Some(&b), &fixture("New")).unwrap(), b);
        // a recorded folder that belongs to another meeting is not reused or renamed
        let d = write(&root, Some(&b), &other).unwrap();
        assert_eq!(d, c);
        assert!(b.join("notes.md").is_file());
        // nor is a symlink, or a folder outside the root
        let link = root.join("link");
        std::os::unix::fs::symlink(&b, &link).unwrap();
        let e = place(&root, Some(&link), &fixture("Linked")).unwrap();
        assert_eq!(e, root.join("2026-10-30 Linked"));
        assert!(link.exists() && b.exists());
    }

    #[test]
    fn links_are_encoded_and_escaped() {
        let mut e = fixture("Links");
        e.docs = ["C# CV (final) [v2].pdf", "C# CV (final) [v2].pdf"]
            .map(|n| Doc {
                name: n.into(),
                data: vec![],
            })
            .into();
        let s = render(&e);
        assert!(s.contains(
            "- [C# CV (final) \\[v2\\].pdf](documents/C%23%20CV%20%28final%29%20%5Bv2%5D.pdf)\n"
        ));
        assert!(s.contains("(documents/C%23%20CV%20%28final%29%20%5Bv2%5D%20%282%29.pdf)"));
    }

    #[test]
    fn collisions_keep_previous_and_symlinks_are_not_followed() {
        let t = Tmp::new();
        let f = write(&t.0, None, &fixture("Safe")).unwrap();
        // A directory in the way gets a numbered alternative.
        let mut bad = fixture("Safe");
        bad.shots.push(Shot {
            automatic: false,
            taken_at: 1.0,
            question: String::new(),
            mime: "image/png".into(),
            data: vec![9],
        });
        fs::create_dir_all(f.join("screenshots/shot-02.png/x")).unwrap();
        fill_tracked(&f, &bad, None).unwrap();
        assert!(f.join("screenshots/shot-02-1.png").is_file());
        assert!(f.join("screenshots/shot-01.png").is_file());
        assert_eq!(
            fs::read_dir(f.join("screenshots"))
                .unwrap()
                .filter(|e| e
                    .as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".tmp"))
                .count(),
            0
        );
        fs::remove_dir_all(f.join("screenshots/shot-02.png")).unwrap();
        // a symlink at the destination is unlinked, its target untouched
        let target = t.0.join("target");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("keep"), "k").unwrap();
        fs::remove_dir_all(f.join("screenshots")).unwrap();
        std::os::unix::fs::symlink(&target, f.join("screenshots")).unwrap();
        fill_tracked(&f, &bad, None).unwrap();
        assert!(target.join("keep").is_file() && f.join("screenshots/shot-02.png").is_file());
        assert_eq!(fs::read_dir(&target).unwrap().count(), 1);
        // A planted symlink at a file name is preserved and gets a numbered alternative.
        fs::remove_file(f.join("screenshots/shot-01.png")).unwrap();
        std::os::unix::fs::symlink(target.join("keep"), f.join("screenshots/shot-01.png")).unwrap();
        std::os::unix::fs::symlink(&target, f.join(".notes.md.tmp")).unwrap();
        fill_tracked(&f, &bad, None).unwrap();
        assert_eq!(fs::read_to_string(target.join("keep")).unwrap(), "k");
        assert_eq!(fs::read_dir(&target).unwrap().count(), 1);
        assert!(
            fs::symlink_metadata(f.join("screenshots/shot-01.png"))
                .unwrap()
                .is_symlink()
        );
        assert!(f.join("screenshots/shot-01-1.png").is_file());
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
            f.join("screenshots"),
            f.join("documents"),
        ] {
            assert_eq!(mode(&d), 0o700, "{d:?}");
        }
        for p in [
            f.join("notes.md"),
            f.join("screenshots/shot-01.png"),
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
        assert!(first.join("screenshots").is_dir() && first.join("documents").is_dir());
        let mut bare = fixture("Sync");
        bare.shots.clear();
        bare.docs.clear();
        // Owned screenshots are removed; other files in their folder still stay.
        fs::remove_file(first.join("documents/cv.pdf")).unwrap();
        let again = write(&t.0, Some(&first), &bare).unwrap();
        assert_eq!(again, first);
        assert!(!again.join("screenshots/shot-01.png").exists());
        assert!(!again.join("documents").exists());
    }

    #[test]
    fn edited_notes_survive_beside_updated_notes() {
        let t = Tmp::new();
        let f = place(&t.0, None, &fixture("Sync")).unwrap();
        let first = fill_tracked(&f, &fixture("Sync"), None).unwrap();
        assert_eq!(first, sha(&fs::read(f.join("notes.md")).unwrap()));
        // unchanged since our write: replaced in place
        let mut later = fixture("Sync");
        later.summary = "Second pass.".into();
        let second = fill_tracked(&f, &later, Some(&first)).unwrap();
        assert!(
            fs::read_to_string(f.join("notes.md"))
                .unwrap()
                .contains("Second pass.")
        );
        assert!(!f.join("notes (updated).md").exists());
        assert_eq!(second, sha(&fs::read(f.join("notes.md")).unwrap()));
        // edited by the user: left alone, the new notes go beside it
        fs::write(f.join("notes.md"), "my edit").unwrap();
        later.summary = "Third pass.".into();
        let third = fill_tracked(&f, &later, Some(&second)).unwrap();
        assert_eq!(fs::read_to_string(f.join("notes.md")).unwrap(), "my edit");
        let updated = fs::read(f.join("notes (updated).md")).unwrap();
        assert!(String::from_utf8_lossy(&updated).contains("Third pass."));
        assert_eq!(third, sha(&updated));
        // the updated file edited too: kept, and the next notes take a new name
        fs::write(f.join("notes (updated).md"), "my second edit").unwrap();
        later.summary = "Fourth pass.".into();
        fill_tracked(&f, &later, Some(&third)).unwrap();
        assert_eq!(
            fs::read_to_string(f.join("notes (updated).md")).unwrap(),
            "my second edit"
        );
        assert!(
            fs::read_to_string(f.join("notes (updated 2).md"))
                .unwrap()
                .contains("Fourth pass.")
        );
        // no stored hash (an export from before hashes were kept): never overwritten
        let g = place(&t.0, None, &fixture("Legacy")).unwrap();
        fs::write(g.join("notes.md"), "old export").unwrap();
        fill_tracked(&g, &fixture("Legacy"), None).unwrap();
        assert_eq!(
            fs::read_to_string(g.join("notes.md")).unwrap(),
            "old export"
        );
        assert!(g.join("notes (updated).md").is_file());
    }

    #[test]
    fn user_files_in_export_folders_survive() {
        let t = Tmp::new();
        let f = write(&t.0, None, &fixture("Sync")).unwrap();
        fs::write(f.join("screenshots/mine.png"), "mine").unwrap();
        fs::write(f.join("documents/extra.txt"), "extra").unwrap();
        let mut bare = fixture("Sync");
        bare.shots.clear();
        bare.docs.clear();
        write(&t.0, Some(&f), &bare).unwrap();
        write(&t.0, Some(&f), &fixture("Sync")).unwrap();
        assert_eq!(
            fs::read_to_string(f.join("screenshots/mine.png")).unwrap(),
            "mine"
        );
        assert_eq!(
            fs::read_to_string(f.join("documents/extra.txt")).unwrap(),
            "extra"
        );
        assert_eq!(
            fs::read(f.join("screenshots/shot-01.png")).unwrap(),
            [1, 2, 3]
        );
    }
    #[test]
    fn automatic_files_are_named_and_only_owned_stale_files_are_removed() {
        let t = Tmp::new();
        let mut e = fixture("Auto");
        e.shots.push(Shot {
            automatic: true,
            taken_at: 70.0,
            question: "[auto]".into(),
            mime: "image/png".into(),
            data: vec![4, 5],
        });
        let f = write(&t.0, None, &e).unwrap();
        assert!(f.join("screenshots/auto-01.png").is_file());
        assert!(
            fs::read_to_string(f.join("notes.md"))
                .unwrap()
                .contains("screenshots/auto-01.png")
        );
        fs::write(f.join("screenshots/auto-99.png"), "user").unwrap();
        e.shots.pop();
        fill_tracked(&f, &e, None).unwrap();
        assert!(!f.join("screenshots/auto-01.png").exists());
        assert_eq!(
            fs::read(f.join("screenshots/auto-99.png")).unwrap(),
            b"user"
        );
        fs::write(f.join("screenshots/auto-01.png"), "also user").unwrap();
        e.shots.push(Shot {
            automatic: true,
            taken_at: 70.0,
            question: "[auto]".into(),
            mime: "image/png".into(),
            data: vec![9],
        });
        fill_tracked(&f, &e, None).unwrap();
        assert_eq!(
            fs::read(f.join("screenshots/auto-01.png")).unwrap(),
            b"also user"
        );
        assert_eq!(fs::read(f.join("screenshots/auto-01-1.png")).unwrap(), [9]);
    }

    #[test]
    fn screenshot_ownership_survives_edits_missing_rows_and_partial_exports() {
        let t = Tmp::new();
        let folder = t.0.join("meeting");
        fs::create_dir(&folder).unwrap();
        let shot = |automatic, data: &[u8]| Shot {
            automatic,
            taken_at: 1.0,
            question: String::new(),
            mime: "image/png".into(),
            data: data.to_vec(),
        };
        let shots = [shot(false, b"manual"), shot(true, b"auto")];
        sync_shots(&folder, &shots).unwrap();
        assert_eq!(
            fs::read_to_string(folder.join("screenshots/.owned-files"))
                .unwrap()
                .lines()
                .count(),
            2
        );
        fs::write(folder.join("screenshots/auto-01.png"), "user edit").unwrap();
        let names = sync_shots(&folder, &shots).unwrap();
        assert_eq!(names, ["shot-01.png", "auto-01-1.png"]);
        fs::remove_file(folder.join("screenshots/.owned-files")).unwrap();
        let names = sync_shots(&folder, &shots).unwrap();
        assert_eq!(names, ["shot-01.png", "auto-01-1.png"]);
        assert_eq!(
            fs::read(folder.join("screenshots/auto-01.png")).unwrap(),
            b"user edit"
        );
        sync_shots(&folder, &[]).unwrap();
        assert!(!folder.join("screenshots/shot-01.png").exists());
        assert!(!folder.join("screenshots/auto-01-1.png").exists());
        assert_eq!(
            fs::read(folder.join("screenshots/auto-01.png")).unwrap(),
            b"user edit"
        );
        fs::create_dir(folder.join("screenshots/auto-01-1.png")).unwrap();
        fs::write(folder.join("screenshots/auto-99.png"), "user").unwrap();
        let names = sync_shots(&folder, &[shot(true, b"new")]).unwrap();
        assert_eq!(names, ["auto-01-2.png"]);
        assert!(folder.join("screenshots/auto-01-1.png").is_dir());
        assert_eq!(
            fs::read(folder.join("screenshots/auto-99.png")).unwrap(),
            b"user"
        );
        let two = [shot(true, b"first"), shot(true, b"second")];
        fs::create_dir(folder.join("screenshots/auto-02.png")).unwrap();
        let names = sync_shots(&folder, &two).unwrap();
        assert_eq!(names, ["auto-01-2.png", "auto-02-1.png"]);
        assert!(folder.join("screenshots/auto-02.png").is_dir());
    }
}
