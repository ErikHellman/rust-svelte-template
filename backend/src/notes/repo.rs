use crate::error::AppResult;
use serde::Serialize;
use sqlx::SqlitePool;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Note {
    pub id: String,
    pub user_id: String,
    pub title: String,
    pub body: String,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn list(pool: &SqlitePool, user_id: &str) -> AppResult<Vec<Note>> {
    let rows = sqlx::query_as!(
        Note,
        r#"SELECT id as "id!", user_id as "user_id!", title as "title!", body as "body!", created_at as "created_at!", updated_at as "updated_at!"
           FROM notes WHERE user_id = ? ORDER BY updated_at DESC"#,
        user_id
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get(pool: &SqlitePool, user_id: &str, id: &str) -> AppResult<Option<Note>> {
    let row = sqlx::query_as!(
        Note,
        r#"SELECT id as "id!", user_id as "user_id!", title as "title!", body as "body!", created_at as "created_at!", updated_at as "updated_at!"
           FROM notes WHERE user_id = ? AND id = ?"#,
        user_id,
        id
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn create(pool: &SqlitePool, user_id: &str, title: &str, body: &str) -> AppResult<Note> {
    let id = uuid::Uuid::now_v7().to_string();
    sqlx::query!(
        "INSERT INTO notes (id, user_id, title, body) VALUES (?, ?, ?, ?)",
        id,
        user_id,
        title,
        body
    )
    .execute(pool)
    .await?;
    Ok(get(pool, user_id, &id).await?.expect("just inserted"))
}

pub async fn update(
    pool: &SqlitePool,
    user_id: &str,
    id: &str,
    title: &str,
    body: &str,
) -> AppResult<Option<Note>> {
    let res = sqlx::query!(
        "UPDATE notes SET title = ?, body = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
         WHERE user_id = ? AND id = ?",
        title,
        body,
        user_id,
        id
    )
    .execute(pool)
    .await?;
    if res.rows_affected() == 0 {
        return Ok(None);
    }
    get(pool, user_id, id).await
}

pub async fn delete(pool: &SqlitePool, user_id: &str, id: &str) -> AppResult<bool> {
    let res = sqlx::query!(
        "DELETE FROM notes WHERE user_id = ? AND id = ?",
        user_id,
        id
    )
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}
