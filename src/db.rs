use crate::model::{ClocData, Project};
use sqlx::{Error, PgPool};

pub async fn create_database_if_not_exists(db_url: &str) -> Result<(), Error> {
    let pool = PgPool::connect(db_url).await?;

    sqlx::query!(
        r#"
        CREATE TABLE IF NOT EXISTS project (
            id SERIAL PRIMARY KEY,
            "user" VARCHAR NOT NULL,
            project_name VARCHAR NOT NULL,
            title VARCHAR NOT NULL,
            ignored_dirs VARCHAR NULL,
            ignored_langs VARCHAR NULL
        );
        "#
    )
    .execute(&pool)
    .await?;

    sqlx::query!(
        r#"
        CREATE TABLE IF NOT EXISTS project_language_stat (
            project_id INT REFERENCES project(id) ON DELETE CASCADE,
            language VARCHAR NOT NULL,
            files INT NOT NULL,
            total_lines INT NOT NULL
        );
        "#
    )
    .execute(&pool)
    .await?;

    Ok(())
}

pub async fn save_project_stats(
    db_url: &str,
    github_user: &str,
    project_name: &str,
    cloc_result: &ClocData,
) -> Result<(), Error> {
    let pool = PgPool::connect(db_url).await?;

    let mut tx = pool.begin().await?;

    let project_record = sqlx::query!(
        r#"
        SELECT id FROM project
        WHERE "user" = $1 AND project_name = $2
        "#,
        github_user,
        project_name
    )
    .fetch_optional(&mut *tx)
    .await?;

    let project_id: i32 = if let Some(record) = project_record {
        record.id
    } else {
        let rec = sqlx::query!(
            r#"
            INSERT INTO project ("user", project_name)
            VALUES ($1, $2)
            RETURNING id
            "#,
            github_user,
            project_name
        )
        .fetch_one(&mut *tx)
        .await?;
        rec.id
    };

    // Remove any existing language stats for this project.
    sqlx::query!(
        "DELETE FROM project_language_stat WHERE project_id = $1",
        project_id
    )
    .execute(&mut *tx)
    .await?;

    for (language, stats) in &cloc_result.languages {
        if language == "SUM" {
            continue;
        }
        sqlx::query!(
            r#"
            INSERT INTO project_language_stat (project_id, language, files, total_lines)
            VALUES ($1, $2, $3, $4)
            "#,
            project_id,
            language,
            stats.n_files as i32,
            stats.total_lines() as i32
        )
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    Ok(())
}

pub async fn get_all_projects(db_url: &str) -> Result<Vec<Project>, Error> {
    let pool = PgPool::connect(db_url).await?;

    let projects = sqlx::query_as!(
        Project,
        r#"
        SELECT "user" AS "github_user!", project_name, title, ignored_dirs, ignored_langs
        FROM project
        "#
    )
    .fetch_all(&pool)
    .await?;

    Ok(projects)
}

#[cfg(test)]
mod tests {
    use crate::db::{create_database_if_not_exists, get_all_projects};

    #[tokio::test]
    async fn test_db() {
        let db = "postgresql://pstatool:pstatool@127.0.0.1:5433/pstatool";
        let result = create_database_if_not_exists(&db).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_all_projects() {
        let url = "postgresql://pstatool:pstatool@127.0.0.1:5433/pstatool";
        let result = get_all_projects(url).await;
        assert!(result.is_ok());
        println!("{:?}", result.unwrap());
    }
}
