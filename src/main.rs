mod db;
mod model;
mod svg;

use crate::model::{ClocConfig, ClocData, Project};

use clap::{arg, Parser};
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use tokio::fs::remove_dir_all;

use clap_derive::Parser;
use log::LevelFilter;
use simple_logger::SimpleLogger;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// PostgreSQL Database URL: postgresql://user:password@host:port/database (or set DB_URL env variable)
    #[arg(long, env = "DB_URL")]
    db_url: String,

    /// Path to the SVG folder (or set SVG_FOLDER env variable)
    #[arg(long, env = "SVG_FOLDER")]
    svg_folder: PathBuf,

    /// Path to the temporary folder to store repositories (or set TEMP_FOLDER env variable)
    #[arg(long, env = "TEMP_FOLDER")]
    temp_folder: PathBuf,
}

#[tokio::main]
async fn main() {
    SimpleLogger::new()
        .with_level(LevelFilter::Debug)
        .with_module_level("sqlx", LevelFilter::Warn)
        .init()
        .expect("Failed to init logger");

    // Parse command line arguments (or fallback to env variables)
    let args = Args::parse();

    log::info!("Updating all projects...");
    // Ensure the database exists before processing
    if let Err(e) = db::create_database_if_not_exists(&args.db_url).await {
        log::error!("Failed to ensure database exists: {}", e);
        return;
    }

    // Pass the values from the command line arguments
    process_all_projects(&args.db_url, &args.svg_folder, &args.temp_folder).await;
}

async fn process_all_projects(db_url: &str, svg_folder: &Path, temp_folder: &Path) {
    match db::get_all_projects(db_url).await {
        Ok(projects) => {
            for project in projects {
                process_project(&project, svg_folder, temp_folder, Some(db_url)).await;
            }
        }
        Err(e) => log::error!("Failed to fetch projects: {}", e),
    }
}

pub fn create_cloc_config(project: &Project, path: &Path) -> ClocConfig {
    let mut ignored_dirs: Vec<String> = vec!["target", ".idea", ".git", ".build"]
        .into_iter()
        .map(String::from)
        .collect();

    let mut ignored_langs: Vec<String> = vec![
        "JSON",
        "Markdown",
        "Maven",
        "Properties",
        "SVG",
        "TOML",
        "XML",
        "YAML",
    ]
    .into_iter()
    .map(String::from)
    .collect();

    if let Some(proj_dirs) = project.ignored_dirs.as_ref() {
        ignored_dirs.extend(proj_dirs.split(',').map(|s| s.trim().to_string()));
    }
    ignored_dirs.dedup();

    if let Some(proj_langs) = project.ignored_langs.as_ref() {
        ignored_langs.extend(proj_langs.split(',').map(|s| s.trim().to_string()));
    }
    ignored_langs.dedup();

    ClocConfig {
        path: path.to_path_buf(),
        ignored_langs,
        ignored_dirs,
    }
}

pub async fn process_project(
    project: &Project,
    svg_folder: &Path,
    temp_folder: &Path,
    db_url: Option<&str>,
) {
    log::trace!(
        "Cloning project {}/{}",
        project.github_user,
        project.project_name
    );
    let repo_url = format!(
        "https://github.com/{}/{}.git",
        project.github_user, project.project_name
    );
    let project_path = temp_folder.join(project.project_name.clone());

    // Clone the repository
    if let Err(e) = clone_repo(&repo_url, &project_path) {
        log::error!("Failed to clone repository: {}", e);
        return;
    }

    let config = create_cloc_config(project, &project_path);

    // Run CLOC on the cloned repository
    match run_cloc(config) {
        Ok(cloc_data) => {
            log::trace!(
                "Generating SVG file for {}/{}",
                project.github_user,
                project.project_name
            );

            // Generate svg
            if let Ok(svg) = svg::generate_svg(&project.title, &cloc_data) {
                // Write to file
                write_svg_to_output_dir(
                    svg_folder,
                    &project.github_user,
                    &project.project_name,
                    &svg,
                );
            }

            // Save the project stats if an url is set
            if let Some(db_url) = db_url {
                log::trace!(
                    "Saving stats to database for {}/{}",
                    project.github_user,
                    project.project_name
                );

                if let Err(e) = db::save_project_stats(
                    db_url,
                    &project.github_user,
                    &project.project_name,
                    &cloc_data,
                )
                .await
                {
                    log::error!("Failed to save project to database: {}", e);
                }
            }
        }
        Err(e) => {
            log::error!("Failed to clone project: {}", e);
        }
    }

    // Clean up the temporary folder
    if let Err(e) = remove_dir_all(&project_path).await {
        log::error!("Failed to remove temp folder: {}", e);
    }

    log::debug!(
        "Processed project {}/{}",
        project.github_user,
        project.project_name
    );
}

pub fn clone_repo(repo_url: &str, dest_path: &Path) -> Result<(), git2::Error> {
    let mut fetch_options = git2::FetchOptions::new();
    let mut checkout_builder = git2::build::CheckoutBuilder::new();

    let repo = git2::Repository::init(dest_path)?;
    let mut remote = repo.remote("origin", repo_url)?;

    // Do a shallow clone as any history data is unused
    let callbacks = git2::RemoteCallbacks::new();
    fetch_options.depth(1).remote_callbacks(callbacks);
    remote.fetch(
        &["refs/heads/main:refs/remotes/origin/main"],
        Some(&mut fetch_options),
        None,
    )?;

    let refname = "refs/remotes/origin/main";
    let obj = repo.revparse_single(refname)?;
    repo.reset(&obj, git2::ResetType::Hard, Some(&mut checkout_builder))?;

    Ok(())
}

pub fn run_cloc(config: ClocConfig) -> Result<ClocData, Box<dyn std::error::Error>> {
    log::trace!("Running cloc with configuration: {:?}", config);
    let ignored_dirs = config.ignored_dirs.join(",");
    let ignored_langs = config.ignored_langs.join(",");

    let mut command = Command::new("cloc");

    let mut args = command.arg("--json");

    if ignored_langs.len() > 0 {
        args = args.arg(format!("--exclude-lang={}", ignored_langs))
    }

    if ignored_dirs.len() > 0 {
        args = args.arg(format!("--exclude-dir={}", ignored_dirs))
    }

    let output = args
        .arg(
            config
                .path
                .to_str()
                .ok_or_else(|| "Invalid repository path".to_string())?,
        )
        .output()?;

    if !output.status.success() {
        return Err(format!("cloc failed: {}", String::from_utf8_lossy(&output.stderr)).into());
    }

    let mut cloc_result: ClocData = serde_json::from_slice(&output.stdout)?;

    cloc_result
        .languages
        .retain(|lang, _| !lang.eq_ignore_ascii_case("sum"));

    Ok(cloc_result)
}

pub fn write_svg_to_output_dir(folder: &Path, user: &str, project_name: &str, contents: &str) {
    let subfolder_path = folder.join(user);
    if !subfolder_path.exists() {
        fs::create_dir_all(&subfolder_path).expect("Failed to create subfolder");
    }
    let svg_file = subfolder_path.join(format!("{}.svg", project_name));

    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(svg_file)
        .expect("Unable to create or open file");

    file.write_all(contents.as_bytes())
        .expect("Unable to write data");
}

#[cfg(test)]
mod tests {
    use crate::db::save_project_stats;
    use crate::model::{ClocConfig, Project};
    use crate::{create_cloc_config, process_project, run_cloc};
    use log::LevelFilter;
    use simple_logger::SimpleLogger;
    use std::path::Path;

    #[tokio::test]
    async fn test_project_generation() {
        let temp_folder = Path::new("/Users/wesley/tmp/");
        let svg_folder = Path::new("/Users/wesley/workspace/project-stats/assets/output/");
        let db = "postgresql://pstatool:pstatool@127.0.0.1:5433/pstatool";

        let project = Project {
            github_user: "wdudokvanheel".to_string(),
            project_name: "babycare".to_string(),
            title: "Baby Care".to_string(),
            ignored_dirs: None,
            ignored_langs: None,
        };

        process_project(&project, svg_folder, temp_folder, Some(db)).await;
    }

    #[tokio::test]
    async fn test_process() {
        setup_test_logger();

        let temp_folder = Path::new("/Users/wesley/tmp/");
        let svg = Path::new("./assets/output/");
        let project_folder = Path::new("/Users/wesley/workspace/babycare/");
        let project = Project {
            github_user: "wdudokvanheel".to_string(),
            project_name: "chip8".to_string(),
            title: "Chip 8 Emu".to_string(),
            ignored_dirs: Some("BabyCare.xcodeproj,Assets.xcassets".to_string()),
            ignored_langs: Some("Lua".to_string()),
        };
        let config = create_cloc_config(&project, project_folder);

        let cloc_data = run_cloc(config);
        assert!(cloc_data.is_ok());
        let cloc_data = cloc_data.unwrap();
        println!("{}", serde_json::to_string_pretty(&cloc_data).unwrap());
    }

    #[tokio::test]
    async fn test_manual_process() {
        let dest = Path::new("/Users/wesley/workspace/babycare/");
        let url = "postgresql://pstatool:pstatool@127.0.0.1:5433/pstatool";

        let ignored = [
            "target",
            ".idea",
            "*.framework",
            "*.xcodeproj",
            "assets",
            "pkg",
        ];

        let config = ClocConfig {
            path: dest.to_path_buf(),
            ignored_langs: vec![],
            ignored_dirs: vec![],
        };

        let result = run_cloc(config).unwrap();

        println!("{}", serde_json::to_string_pretty(&result).unwrap());

        save_project_stats(url, "wdudokvanheel", "baby-care", &result)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_run_cloc() {
        setup_test_logger();
        let dest = Path::new("/Users/wesley/workspace/babycare/");
        let url = "postgresql://pstatool:pstatool@127.0.0.1:5433/pstatool";

        let ignored = ["target", ".idea"];

        let config = ClocConfig {
            path: dest.to_path_buf(),
            ignored_langs: vec!["TOML".to_string()],
            ignored_dirs: ignored.iter().map(|s| s.to_string()).collect(),
        };

        let result = run_cloc(config);
        assert!(result.is_ok());

        println!(
            "{}",
            serde_json::to_string_pretty(&result.unwrap()).unwrap()
        );
    }

    #[test]
    fn test_cloc_config_gen() {
        let dest = Path::new("/Users/wesley/workspace/babycare/");
        let project = Project {
            github_user: "wdudokvanheel".to_string(),
            project_name: "chip8".to_string(),
            title: "Chip 8 Emu".to_string(),
            ignored_dirs: Some("testa,testb".to_string()),
            ignored_langs: Some("Swift,Rust".to_string()),
        };
        let config = create_cloc_config(&project, dest);

        assert!(config.ignored_langs.contains(&"Properties".to_string()));
        assert!(config.ignored_langs.contains(&"TOML".to_string()));
        assert!(config.ignored_langs.contains(&"Swift".to_string()));
        assert!(config.ignored_langs.contains(&"Rust".to_string()));
        assert!(!config.ignored_langs.contains(&"HTML".to_string()));

        assert!(config.ignored_dirs.contains(&"testa".to_string()));
        assert!(config.ignored_dirs.contains(&"testb".to_string()));
        assert!(!config.ignored_dirs.contains(&"testc".to_string()));
    }

    fn setup_test_logger() {
        SimpleLogger::new()
            .with_level(LevelFilter::Trace)
            .with_module_level("sqlx", LevelFilter::Warn)
            .init()
            .expect("Failed to init logger");
    }
}
