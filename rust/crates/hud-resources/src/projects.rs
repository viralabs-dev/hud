//! Explicit, bounded, read-only project discovery; no symlink traversal.
use std::{io, path::Path};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    pub name: String,
    pub folder: String,
    pub kanban: Option<String>,
}
#[cfg(unix)]
pub fn discover(root: &Path) -> io::Result<Vec<Project>> {
    use rustix::fs::{self, AtFlags, Dir, FileType, Mode, OFlags};
    use std::{os::fd::OwnedFd, path::Component};
    fn flags() -> OFlags {
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC
    }
    fn listing(fd: &OwnedFd, entries: &mut usize) -> io::Result<(Vec<String>, Vec<String>)> {
        let mut dirs = Vec::new();
        let mut files = Vec::new();
        for entry in Dir::read_from(fd)? {
            *entries += 1;
            if *entries > 20_000 {
                break;
            }
            let entry = entry?;
            let Ok(name) = entry.file_name().to_str() else {
                continue;
            };
            if name.starts_with('.')
                || [
                    "node_modules",
                    "__pycache__",
                    "venv",
                    "target",
                    "dist",
                    "build",
                    "vendor",
                ]
                .contains(&name)
            {
                continue;
            }
            let info = fs::statat(fd, name, AtFlags::SYMLINK_NOFOLLOW)?;
            match FileType::from_raw_mode(info.st_mode) {
                FileType::Directory => dirs.push(name.to_owned()),
                FileType::RegularFile => files.push(name.to_owned()),
                _ => {}
            }
        }
        dirs.sort();
        files.sort();
        Ok((dirs, files))
    }
    if root.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mut fd = fs::open(
        if root.is_absolute() {
            Path::new("/")
        } else {
            Path::new(".")
        },
        flags(),
        Mode::empty(),
    )?;
    for component in root.components() {
        if let Component::Normal(name) = component {
            fd = fs::openat(&fd, name, flags(), Mode::empty())?;
        }
    }
    fn walk(
        fd: OwnedFd,
        relative: String,
        depth: usize,
        projects: &mut Vec<Project>,
        entries: &mut usize,
    ) -> io::Result<()> {
        if *entries >= 20_000 || projects.len() >= 200 {
            return Ok(());
        }
        let (dirs, files) = listing(&fd, entries)?;
        let mut project = None;
        if !relative.is_empty() {
            for directory in dirs
                .iter()
                .filter(|name| name.to_lowercase().contains("backlog"))
            {
                let Ok(backlog) = fs::openat(&fd, directory.as_str(), flags(), Mode::empty())
                else {
                    continue;
                };
                for file in listing(&backlog, entries)?.1 {
                    if let Some(name) = file
                        .strip_prefix("Kanban (")
                        .and_then(|s| s.strip_suffix(").md"))
                        .filter(|s| !s.is_empty())
                    {
                        project = Some(Project {
                            name: name.into(),
                            folder: relative.clone(),
                            kanban: Some(format!("{relative}/{directory}/{file}")),
                        });
                        break;
                    }
                }
                if project.is_some() {
                    break;
                }
            }
            if project.is_none()
                && dirs.iter().any(|name| {
                    name.as_bytes().get(0..3).is_some_and(|p| {
                        p[0].is_ascii_digit() && p[1].is_ascii_digit() && p[2] == b'-'
                    }) && name.len() > 3
                })
            {
                let leaf = relative.rsplit('/').next().unwrap_or("");
                if let Some(file) = files
                    .iter()
                    .find(|file| file.to_lowercase() == format!("{leaf}.md").to_lowercase())
                {
                    project = Some(Project {
                        name: file[..file.len() - 3].into(),
                        folder: relative.clone(),
                        kanban: None,
                    });
                }
            }
        }
        if let Some(project) = project {
            projects.push(project);
            return Ok(());
        }
        if depth < 5 {
            for name in dirs {
                if let Ok(child) = fs::openat(&fd, name.as_str(), flags(), Mode::empty()) {
                    let relative = if relative.is_empty() {
                        name
                    } else {
                        format!("{relative}/{name}")
                    };
                    walk(child, relative, depth + 1, projects, entries)?;
                }
            }
        }
        Ok(())
    }
    let mut projects = Vec::new();
    walk(fd, String::new(), 0, &mut projects, &mut 0)?;

    projects.sort_by(|a, b| a.folder.cmp(&b.folder));
    Ok(projects)
}
#[cfg(not(unix))]
pub fn discover(_: &Path) -> io::Result<Vec<Project>> {
    Err(io::ErrorKind::Unsupported.into())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[test]
    fn synthetic_project_shapes_skip_system_artifacts_links_and_nested_projects() {
        let root = std::env::temp_dir().join(format!("hud-project-fixture-{}", std::process::id()));
        for folder in [
            "group/Hud/06-backlog",
            "group/Hud/nested/06-backlog",
            "Other/01-arquitetura",
            ".hidden/06-backlog",
            "node_modules/dependency/06-backlog",
        ] {
            std::fs::create_dir_all(root.join(folder)).unwrap();
        }
        for file in [
            "group/Hud/06-backlog/Kanban (Hud).md",
            "group/Hud/nested/06-backlog/Kanban (Nested).md",
            "Other/Other.md",
            ".hidden/06-backlog/Kanban (Hidden).md",
            "node_modules/dependency/06-backlog/Kanban (Dependency).md",
        ] {
            std::fs::write(root.join(file), "fixture").unwrap();
        }
        std::os::unix::fs::symlink(root.join("Other"), root.join("link")).unwrap();
        let projects = discover(&root).unwrap();
        assert_eq!(
            projects.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            vec!["Other", "Hud"]
        );
        assert_eq!(projects[1].folder, "group/Hud");
        let expected: serde_json::Value =
            serde_json::from_str(include_str!("fixtures/python-project-discovery.json")).unwrap();
        let actual = serde_json::Value::Array(
            projects
                .iter()
                .map(|p| serde_json::json!({"name":p.name,"folder":p.folder,"kanban":p.kanban}))
                .collect(),
        );
        assert_eq!(actual, expected);
        std::fs::remove_dir_all(root).unwrap();
    }
}
