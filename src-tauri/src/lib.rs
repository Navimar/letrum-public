use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use tauri::{AppHandle, Manager};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectFile {
    path: String,
    relative_path: String,
    content: String,
}

#[derive(Deserialize)]
struct SaveFile {
    path: String,
    content: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateFilePayload {
    folder_path: String,
    relative_path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteFilePayload {
    folder_path: String,
    path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenameFilePayload {
    folder_path: String,
    path: String,
    title: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReorderFilesPayload {
    folder_path: String,
    ordered_paths: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateAndInsertFilePayload {
    folder_path: String,
    ordered_paths: Vec<String>,
    selected_paths: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExtractSelectionToFilePayload {
    folder_path: String,
    ordered_paths: Vec<String>,
    source_path: String,
    files: Vec<SaveFile>,
    extracted_content: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PathMapping {
    old_path: String,
    new_path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReorderResult {
    files: Vec<ProjectFile>,
    path_map: Vec<PathMapping>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SaveResult {
    files: Vec<ProjectFile>,
    path_map: Vec<PathMapping>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateAndInsertResult {
    files: Vec<ProjectFile>,
    path_map: Vec<PathMapping>,
    created_path: String,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct AppSettings {
    last_opened_folder: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
struct SceneMetadata {
    emoji: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
struct ProjectMetadata {
    scenes: HashMap<String, SceneMetadata>,
}

const AUTO_TITLE_MAX_CHARS: usize = 48;
const PROJECT_AGENTS_MD: &str = r#"# Letrum Manuscript Project

This folder is managed by the Letrum manuscript editor.

The markdown and text files in the project root are ordered scene files, not a
generic notes folder. Preserve the existing numbered filename structure and do
not create extra folders, move scenes into subdirectories, or rename files
outside the editor's numbering/title conventions unless the user explicitly asks.

Scene notes live in the visible `letrum_notes/` folder. Treat files named like
`001_scene-name_note.md` as notes for the matching scene file, not as manuscript
scenes. You may read and edit them when the task asks about notes, plans,
continuity, outlines, or scene-specific context, but do not include them in the
manuscript order and do not rename them independently from their scene.

Project metadata lives in `.letrum/`. Do not edit `.letrum/scenes.json` by hand
unless the task is specifically about Letrum metadata.
"#;

fn strip_numeric_prefix(name: &str) -> &str {
    let bytes = name.as_bytes();
    let mut index = 0;

    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }

    if index > 0
        && index < bytes.len()
        && (bytes[index] == b'_' || bytes[index] == b'-' || bytes[index] == b' ')
    {
        index += 1;
    }

    if index > 0 && index < bytes.len() {
        &name[index..]
    } else {
        name
    }
}

fn numeric_prefix(name: &str) -> &str {
    let bytes = name.as_bytes();
    let mut index = 0;

    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }

    if index > 0
        && index < bytes.len()
        && (bytes[index] == b'_' || bytes[index] == b'-' || bytes[index] == b' ')
    {
        index += 1;
    }

    &name[..index]
}

fn is_standard_generated_title(title: &str) -> bool {
    title == "new-scene"
}

fn truncate_title(title: &str, max_chars: usize) -> String {
    title.chars().take(max_chars).collect::<String>()
}

fn sanitized_auto_title(content: &str) -> Option<String> {
    let normalized = content.replace("\r\n", "\n");
    let first_sentence = normalized.split('.').next()?.trim();

    if first_sentence.is_empty() {
        return None;
    }

    let mut title = String::new();
    let mut previous_was_space = false;

    for character in truncate_title(first_sentence, AUTO_TITLE_MAX_CHARS).chars() {
        let safe_character = match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => ' ',
            character if character.is_control() => ' ',
            character => character,
        };

        if safe_character.is_whitespace() {
            if !previous_was_space {
                title.push(' ');
                previous_was_space = true;
            }
            continue;
        }

        title.push(safe_character);
        previous_was_space = false;
    }

    let title = title.trim().trim_matches('.').trim().to_string();

    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}

fn unique_path(parent: &Path, file_name: &str, extension: &str, original: &Path) -> PathBuf {
    let mut candidate = parent.join(format!("{}.{}", file_name, extension));

    if candidate == original || !candidate.exists() {
        return candidate;
    }

    let mut index = 2usize;
    loop {
        candidate = parent.join(format!("{} {}.{}", file_name, index, extension));

        if candidate == original || !candidate.exists() {
            return candidate;
        }

        index += 1;
    }
}

fn canonical_project_root(folder_path: &str) -> Result<PathBuf, String> {
    let root = PathBuf::from(folder_path);

    if !root.exists() {
        return Err("Folder does not exist.".into());
    }

    if !root.is_dir() {
        return Err("Path is not a folder.".into());
    }

    root.canonicalize().map_err(|error| error.to_string())
}

fn has_scene_extension(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|value| value.to_str()),
        Some("md") | Some("txt")
    )
}

fn is_agents_file(path: &Path) -> bool {
    path.file_name().and_then(|value| value.to_str()) == Some("AGENTS.md")
}

fn ensure_scene_file_name(path: &Path) -> Result<(), String> {
    if is_agents_file(path) {
        return Err("AGENTS.md is not a scene file.".into());
    }

    if !has_scene_extension(path) {
        return Err("Scene file must be .md or .txt.".into());
    }

    Ok(())
}

fn project_root_child(root: &Path, file_name: &str) -> Result<PathBuf, String> {
    let relative = Path::new(file_name);
    let mut components = relative.components();

    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => {}
        _ => return Err("Scene path must be a file name in the project root.".into()),
    }

    if file_name.trim().is_empty() || file_name.contains('/') || file_name.contains('\\') {
        return Err("Scene path must be a file name in the project root.".into());
    }

    let path = root.join(relative);
    ensure_scene_file_name(&path)?;
    Ok(path)
}

fn project_scene_path(root: &Path, path: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(path);

    if !path.exists() {
        return Err("File does not exist.".into());
    }

    if !path.is_file() {
        return Err("Path is not a file.".into());
    }

    let canonical_path = path.canonicalize().map_err(|error| error.to_string())?;
    if !canonical_path.starts_with(root) {
        return Err("File is outside the project folder.".into());
    }

    let parent = canonical_path
        .parent()
        .ok_or_else(|| "File has no parent directory.".to_string())?;
    if parent != root {
        return Err("Scene file must be in the project root.".into());
    }

    ensure_scene_file_name(&canonical_path)?;
    Ok(canonical_path)
}

fn project_file_from_path(root: &Path, path: &Path) -> Result<ProjectFile, String> {
    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let relative_path = path
        .strip_prefix(root)
        .map_err(|error| error.to_string())?
        .to_string_lossy()
        .replace('\\', "/");

    Ok(ProjectFile {
        path: path.to_string_lossy().into_owned(),
        relative_path,
        content,
    })
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let config_dir = app
        .path()
        .app_config_dir()
        .map_err(|error| error.to_string())?;

    fs::create_dir_all(&config_dir).map_err(|error| error.to_string())?;
    Ok(config_dir.join("settings.json"))
}

fn project_support_dir(root: &Path) -> PathBuf {
    root.join(".letrum")
}

fn project_metadata_path(root: &Path) -> PathBuf {
    project_support_dir(root).join("scenes.json")
}

fn project_notes_dir(root: &Path) -> PathBuf {
    root.join("letrum_notes")
}

fn note_path_for_relative_path(root: &Path, relative_path: &str) -> PathBuf {
    let path = Path::new(relative_path);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.to_string())
        .unwrap_or_else(|| relative_path.replace(['/', '\\'], "__"));
    let file_name = format!("{}_note.md", stem);
    project_notes_dir(root).join(file_name)
}

fn note_path_for_scene_path(root: &Path, scene_path: &Path) -> Result<PathBuf, String> {
    let relative_path = scene_path
        .strip_prefix(root)
        .map_err(|_| "Scene file is outside the project folder.".to_string())?
        .to_string_lossy()
        .replace('\\', "/");

    Ok(note_path_for_relative_path(root, &relative_path))
}

fn project_agents_path(root: &Path) -> PathBuf {
    root.join("AGENTS.md")
}

fn ensure_project_agents_file(root: &Path) -> Result<(), String> {
    let support_dir = project_support_dir(root);
    fs::create_dir_all(&support_dir).map_err(|error| error.to_string())?;

    let agents_path = project_agents_path(root);
    if agents_path.exists() {
        return Ok(());
    }

    fs::write(agents_path, PROJECT_AGENTS_MD).map_err(|error| error.to_string())
}

fn remap_note_files(root: &Path, path_map: &[PathMapping]) -> Result<(), String> {
    for mapping in path_map {
        let old_scene_path = PathBuf::from(&mapping.old_path);
        let new_scene_path = PathBuf::from(&mapping.new_path);

        let new_note_path = note_path_for_scene_path(root, &new_scene_path)?;

        if let Some(parent) = new_note_path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }

        let old_note_path = note_path_for_scene_path(root, &old_scene_path)?;
        if old_note_path == new_note_path || !old_note_path.exists() {
            continue;
        }

        if new_note_path.exists() {
            continue;
        }

        fs::rename(old_note_path, &new_note_path).map_err(|error| error.to_string())?;
    }

    Ok(())
}

fn reorder_files_internal(root: &Path, ordered_paths: &[String]) -> Result<ReorderResult, String> {
    if ordered_paths.is_empty() {
        return Err("No files provided for reorder.".into());
    }

    let width = ordered_paths.len().max(3).to_string().len().max(3);
    let original_paths = ordered_paths
        .iter()
        .map(|path| project_scene_path(root, path))
        .collect::<Result<Vec<_>, _>>()?;
    let mut temporary_paths = Vec::new();
    let mut metadata: HashMap<String, (PathBuf, String)> = HashMap::new();

    for (index, original) in original_paths.iter().enumerate() {
        let file_name = original
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| format!("Invalid file name: {}", original.display()))?;

        let parent = original
            .parent()
            .ok_or_else(|| format!("Invalid parent folder: {}", original.display()))?
            .to_path_buf();

        let clean_name = strip_numeric_prefix(file_name).to_string();
        let temp_path = parent.join(format!(".__tmp_reorder__{:04}_{}", index, file_name));

        fs::rename(original, &temp_path).map_err(|error| error.to_string())?;
        let original_key = original.to_string_lossy().into_owned();
        temporary_paths.push((original_key.clone(), temp_path));
        metadata.insert(original_key, (parent, clean_name));
    }

    let mut path_map = Vec::new();

    for (index, original_path) in original_paths.iter().enumerate() {
        let original_key = original_path.to_string_lossy().into_owned();
        let temp_path = temporary_paths
            .iter()
            .find(|(old_path, _)| old_path == &original_key)
            .map(|(_, temp)| temp.clone())
            .ok_or_else(|| format!("Temporary path missing for {}", original_path.display()))?;

        let (parent, clean_name) = metadata
            .get(&original_key)
            .ok_or_else(|| format!("Metadata missing for {}", original_path.display()))?;

        let new_file_name = format!("{:0width$}_{}", index + 1, clean_name, width = width);
        let new_path = parent.join(new_file_name);
        fs::rename(&temp_path, &new_path).map_err(|error| error.to_string())?;

        path_map.push(PathMapping {
            old_path: original_key,
            new_path: new_path.to_string_lossy().into_owned(),
        });
    }

    remap_note_files(root, &path_map)?;

    let mut files = Vec::new();
    collect_files(root, &mut files)?;
    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));

    Ok(ReorderResult { files, path_map })
}

fn collect_files(root: &Path, results: &mut Vec<ProjectFile>) -> Result<(), String> {
    let entries = fs::read_dir(root).map_err(|error| error.to_string())?;

    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();

        if path.is_dir() {
            continue;
        }

        if is_agents_file(&path) {
            continue;
        }

        if !has_scene_extension(&path) {
            continue;
        }

        let content = fs::read_to_string(&path).map_err(|error| error.to_string())?;
        let relative_path = path
            .strip_prefix(root)
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .replace('\\', "/");

        results.push(ProjectFile {
            path: path.to_string_lossy().into_owned(),
            relative_path,
            content,
        });
    }

    Ok(())
}

#[tauri::command]
fn load_project(folder_path: String) -> Result<Vec<ProjectFile>, String> {
    let root = canonical_project_root(&folder_path)?;

    let mut files = Vec::new();
    collect_files(&root, &mut files)?;
    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(files)
}

#[tauri::command]
fn load_project_metadata(folder_path: String) -> Result<ProjectMetadata, String> {
    let root = canonical_project_root(&folder_path)?;

    let path = project_metadata_path(&root);
    if !path.exists() {
        return Ok(ProjectMetadata::default());
    }

    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    serde_json::from_str(&content).map_err(|error| error.to_string())
}

#[tauri::command]
fn load_scene_note(folder_path: String, scene_path: String) -> Result<String, String> {
    let root = canonical_project_root(&folder_path)?;
    let scene_path = project_scene_path(&root, &scene_path)?;

    let note_path = note_path_for_scene_path(&root, &scene_path)?;
    if !note_path.exists() {
        return Ok(String::new());
    }

    fs::read_to_string(note_path).map_err(|error| error.to_string())
}

#[tauri::command]
fn save_scene_note(folder_path: String, scene_path: String, content: String) -> Result<(), String> {
    let root = canonical_project_root(&folder_path)?;

    ensure_project_agents_file(&root)?;

    let scene_path = project_scene_path(&root, &scene_path)?;

    let note_path = note_path_for_scene_path(&root, &scene_path)?;
    if let Some(parent) = note_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    fs::write(note_path, content).map_err(|error| error.to_string())
}

#[tauri::command]
fn save_project_metadata(folder_path: String, metadata: ProjectMetadata) -> Result<(), String> {
    let root = canonical_project_root(&folder_path)?;

    ensure_project_agents_file(&root)?;

    let path = project_metadata_path(&root);
    let parent = path
        .parent()
        .ok_or_else(|| "Metadata file has no parent directory.".to_string())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;

    let content = serde_json::to_string_pretty(&metadata).map_err(|error| error.to_string())?;
    fs::write(path, content).map_err(|error| error.to_string())
}

#[tauri::command]
fn save_project(folder_path: String, files: Vec<SaveFile>) -> Result<SaveResult, String> {
    let root = canonical_project_root(&folder_path)?;

    ensure_project_agents_file(&root)?;

    let mut saved_files = Vec::new();
    let mut path_map = Vec::new();

    for file in files {
        let original = project_scene_path(&root, &file.path)?;

        let parent = original
            .parent()
            .ok_or_else(|| "File has no parent directory.".to_string())?;

        let file_stem = original
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or_else(|| "Invalid file name.".to_string())?;

        let extension = original
            .extension()
            .and_then(|value| value.to_str())
            .ok_or_else(|| "File has no extension.".to_string())?;

        let mut target_path = original.clone();
        let clean_title = strip_numeric_prefix(file_stem);

        if is_standard_generated_title(clean_title) {
            if let Some(auto_title) = sanitized_auto_title(&file.content) {
                let target_file_name = format!("{}{}", numeric_prefix(file_stem), auto_title);
                target_path = unique_path(parent, &target_file_name, extension, &original);
            }
        }

        if target_path != original {
            fs::rename(&original, &target_path).map_err(|error| error.to_string())?;
            path_map.push(PathMapping {
                old_path: file.path.clone(),
                new_path: target_path.to_string_lossy().into_owned(),
            });
        }

        fs::write(&target_path, file.content).map_err(|error| error.to_string())?;
        saved_files.push(project_file_from_path(&root, &target_path)?);
    }

    remap_note_files(&root, &path_map)?;

    Ok(SaveResult {
        files: saved_files,
        path_map,
    })
}

#[tauri::command]
fn create_file(payload: CreateFilePayload) -> Result<ProjectFile, String> {
    let root = canonical_project_root(&payload.folder_path)?;

    let relative = payload.relative_path.trim();
    if relative.is_empty() {
        return Err("File path cannot be empty.".into());
    }

    let path = project_root_child(&root, relative)?;
    if path.exists() {
        return Err("File already exists.".into());
    }

    fs::write(&path, "").map_err(|error| error.to_string())?;

    Ok(ProjectFile {
        path: path.to_string_lossy().into_owned(),
        relative_path: path
            .strip_prefix(&root)
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .replace('\\', "/"),
        content: String::new(),
    })
}

#[tauri::command]
fn delete_file(payload: DeleteFilePayload) -> Result<(), String> {
    let root = canonical_project_root(&payload.folder_path)?;
    let path = project_scene_path(&root, &payload.path)?;

    let note_path = note_path_for_scene_path(&root, &path)?;
    if note_path.exists() {
        fs::remove_file(note_path).map_err(|error| error.to_string())?;
    }

    fs::remove_file(path).map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
fn rename_file(payload: RenameFilePayload) -> Result<ProjectFile, String> {
    let root = canonical_project_root(&payload.folder_path)?;
    let original = project_scene_path(&root, &payload.path)?;

    let parent = original
        .parent()
        .ok_or_else(|| "File has no parent directory.".to_string())?;

    let file_name = original
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Invalid file name.".to_string())?;

    let extension = original
        .extension()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "File has no extension.".to_string())?;

    let mut title = payload.title.trim().to_string();
    let extension_suffix = format!(".{}", extension);
    if title
        .to_lowercase()
        .ends_with(&extension_suffix.to_lowercase())
    {
        title.truncate(title.len() - extension_suffix.len());
        title = title.trim().to_string();
    }

    if title.is_empty() {
        return Err("File title cannot be empty.".into());
    }

    if title.contains('/') || title.contains('\\') || title == "." || title == ".." {
        return Err("File title cannot contain path separators.".into());
    }

    let new_file_name = format!("{}{}.{}", numeric_prefix(file_name), title, extension);
    let new_path = parent.join(new_file_name);

    if new_path != original && new_path.exists() {
        return Err("File already exists.".into());
    }

    fs::rename(&original, &new_path).map_err(|error| error.to_string())?;

    remap_note_files(
        &root,
        &[PathMapping {
            old_path: original.to_string_lossy().into_owned(),
            new_path: new_path.to_string_lossy().into_owned(),
        }],
    )?;

    let content = fs::read_to_string(&new_path).map_err(|error| error.to_string())?;
    let relative_path = new_path
        .strip_prefix(&root)
        .map_err(|error| error.to_string())?
        .to_string_lossy()
        .replace('\\', "/");

    Ok(ProjectFile {
        path: new_path.to_string_lossy().into_owned(),
        relative_path,
        content,
    })
}

#[tauri::command]
fn reorder_files(payload: ReorderFilesPayload) -> Result<ReorderResult, String> {
    let root = canonical_project_root(&payload.folder_path)?;

    reorder_files_internal(&root, &payload.ordered_paths)
}

#[tauri::command]
fn create_and_insert_file(
    payload: CreateAndInsertFilePayload,
) -> Result<CreateAndInsertResult, String> {
    let root = canonical_project_root(&payload.folder_path)?;
    let ordered_paths = payload
        .ordered_paths
        .iter()
        .map(|path| project_scene_path(&root, path).map(|path| path.to_string_lossy().into_owned()))
        .collect::<Result<Vec<String>, _>>()?;
    let selected_paths = payload
        .selected_paths
        .iter()
        .map(|path| project_scene_path(&root, path).map(|path| path.to_string_lossy().into_owned()))
        .collect::<Result<Vec<String>, _>>()?;

    let insert_after_index = if selected_paths.is_empty() {
        ordered_paths.len()
    } else {
        ordered_paths
            .iter()
            .enumerate()
            .filter(|(_, path)| selected_paths.contains(path))
            .map(|(index, _)| index + 1)
            .max()
            .unwrap_or(ordered_paths.len())
    };

    if !selected_paths
        .iter()
        .all(|path| ordered_paths.contains(path))
    {
        return Err("Selected files are not present in the ordered list.".into());
    }

    let created_path = unique_path(
        &root,
        "new-scene",
        "md",
        &root.join(".__never_matches__.md"),
    );
    fs::write(&created_path, "").map_err(|error| error.to_string())?;

    let created_path_string = created_path.to_string_lossy().into_owned();
    let mut ordered_paths = ordered_paths;
    ordered_paths.insert(insert_after_index, created_path_string.clone());

    let reorder_result = reorder_files_internal(&root, &ordered_paths)?;
    let created_final_path = reorder_result
        .path_map
        .iter()
        .find(|mapping| mapping.old_path == created_path_string)
        .map(|mapping| mapping.new_path.clone())
        .ok_or_else(|| "Created file mapping not found after reorder.".to_string())?;

    Ok(CreateAndInsertResult {
        files: reorder_result.files,
        path_map: reorder_result.path_map,
        created_path: created_final_path,
    })
}

#[tauri::command]
fn extract_selection_to_file(
    payload: ExtractSelectionToFilePayload,
) -> Result<CreateAndInsertResult, String> {
    let root = canonical_project_root(&payload.folder_path)?;

    if payload.extracted_content.is_empty() {
        return Err("No text selected.".into());
    }

    let ordered_paths = payload
        .ordered_paths
        .iter()
        .map(|path| project_scene_path(&root, path).map(|path| path.to_string_lossy().into_owned()))
        .collect::<Result<Vec<String>, _>>()?;
    let source_path = project_scene_path(&root, &payload.source_path)?;
    let source_path_string = source_path.to_string_lossy().into_owned();
    let source_index = ordered_paths
        .iter()
        .position(|path| path == &source_path_string)
        .ok_or_else(|| "Source file is not present in the ordered list.".to_string())?;

    for file in &payload.files {
        let path = project_scene_path(&root, &file.path)?;

        fs::write(path, &file.content).map_err(|error| error.to_string())?;
    }

    let created_path = unique_path(
        &root,
        "new-scene",
        "md",
        &root.join(".__never_matches__.md"),
    );
    fs::write(&created_path, &payload.extracted_content).map_err(|error| error.to_string())?;

    let created_path_string = created_path.to_string_lossy().into_owned();
    let mut ordered_paths = ordered_paths;
    ordered_paths.insert(source_index + 1, created_path_string.clone());

    let reorder_result = reorder_files_internal(&root, &ordered_paths)?;
    let created_final_path = reorder_result
        .path_map
        .iter()
        .find(|mapping| mapping.old_path == created_path_string)
        .map(|mapping| mapping.new_path.clone())
        .ok_or_else(|| "Created file mapping not found after reorder.".to_string())?;

    let mut final_created_path = PathBuf::from(&created_final_path);
    let file_stem = final_created_path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Invalid created file name.".to_string())?;
    let extension = final_created_path
        .extension()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Created file has no extension.".to_string())?;

    let mut path_map = reorder_result.path_map;
    if is_standard_generated_title(strip_numeric_prefix(file_stem)) {
        if let Some(auto_title) = sanitized_auto_title(&payload.extracted_content) {
            let target_file_name = format!("{}{}", numeric_prefix(file_stem), auto_title);
            let target_path = unique_path(
                final_created_path
                    .parent()
                    .ok_or_else(|| "Created file has no parent directory.".to_string())?,
                &target_file_name,
                extension,
                &final_created_path,
            );

            if target_path != final_created_path {
                fs::rename(&final_created_path, &target_path).map_err(|error| error.to_string())?;
                path_map.push(PathMapping {
                    old_path: created_final_path,
                    new_path: target_path.to_string_lossy().into_owned(),
                });
                final_created_path = target_path;
            }
        }
    }

    let mut files = Vec::new();
    collect_files(&root, &mut files)?;
    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));

    Ok(CreateAndInsertResult {
        files,
        path_map,
        created_path: final_created_path.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
fn load_app_settings(app: AppHandle) -> Result<AppSettings, String> {
    let path = settings_path(&app)?;

    if !path.exists() {
        return Ok(AppSettings::default());
    }

    let raw = fs::read_to_string(path).map_err(|error| error.to_string())?;
    serde_json::from_str(&raw).map_err(|error| error.to_string())
}

#[tauri::command]
fn save_app_settings(app: AppHandle, settings: AppSettings) -> Result<(), String> {
    let path = settings_path(&app)?;
    let raw = serde_json::to_string_pretty(&settings).map_err(|error| error.to_string())?;
    fs::write(path, raw).map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            load_project,
            load_project_metadata,
            load_scene_note,
            save_scene_note,
            save_project_metadata,
            save_project,
            create_file,
            create_and_insert_file,
            extract_selection_to_file,
            delete_file,
            rename_file,
            reorder_files,
            load_app_settings,
            save_app_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
