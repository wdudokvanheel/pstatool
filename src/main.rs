mod db;
mod model;
mod svg;

use crate::model::{ClocData, Project};

use clap::Parser;
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
                process_project(&project, svg_folder, temp_folder, db_url).await;
            }
        }
        Err(e) => log::error!("Failed to fetch projects: {}", e),
    }
}

pub async fn process_project(
    project: &Project,
    svg_folder: &Path,
    temp_folder: &Path,
    db_url: &str,
) {
    log::trace!("Cloning project {}/{}", project.github_user, project.project_name);
    let repo_url = format!(
        "https://github.com/{}/{}.git",
        project.github_user, project.project_name
    );
    let project_path = temp_folder.join(project.project_name.clone());

    // TODO Merge these with per-project dirs from the database
    let ignored_dirs = [
        "target",
        ".idea",
        "*.framework",
        "*.xcodeproj",
        "assets",
        "pkg",
    ];

    // Clone the repository
    if let Err(e) = clone_repo(&repo_url, &project_path) {
        log::error!("Failed to clone repository: {}", e);
        return;
    }

    // Run CLOC on the cloned repository
    match run_cloc(&project_path, &ignored_dirs) {
        Ok(cloc_data) => {
            log::trace!("Generating SVG file for {}/{}", project.github_user, project.project_name);
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

            log::trace!("Saving stats to database for {}/{}", project.github_user, project.project_name);
            // Save the project stats
            if let Err(e) = db::save_project_stats(
                db_url,
                &project.github_user,
                &project.project_name,
                &cloc_data,
            )
            .await
            {
                log::error!("Failed to save project: {}", e);
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

    log::debug!("Processed project {}/{}", project.github_user, project.project_name);
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

pub fn run_cloc(
    repo_path: &Path,
    ignored_dirs: &[&str],
) -> Result<ClocData, Box<dyn std::error::Error>> {
    let exclude_dirs = ignored_dirs.join(",");

    let output = Command::new("cloc")
        .arg("--json")
        .arg(format!("--exclude-dir={}", exclude_dirs))
        .arg(
            repo_path
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
    use crate::model::Project;
    use crate::{process_project, run_cloc};
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
        };

        process_project(&project, svg_folder, temp_folder, &db).await;
    }

    #[tokio::test]
    async fn test_run_cloc_and_save() {
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
        let result = run_cloc(dest, &ignored).unwrap();

        println!("{}", serde_json::to_string_pretty(&result).unwrap());

        save_project_stats(url, "wdudokvanheel", "baby-care", &result)
            .await
            .unwrap();
    }
}
