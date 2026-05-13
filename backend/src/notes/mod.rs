pub mod repo;

use crate::AppState;
use crate::auth::middleware::AuthUser;
use crate::error::{AppError, AppResult};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_notes).post(create_note))
        .route("/{id}", get(get_note).put(update_note).delete(delete_note))
}

#[derive(Deserialize)]
struct NoteInput {
    title: String,
    #[serde(default)]
    body: String,
}

async fn list_notes(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
) -> AppResult<Json<Vec<repo::Note>>> {
    Ok(Json(repo::list(&state.db, &user_id).await?))
}

async fn get_note(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<repo::Note>> {
    repo::get(&state.db, &user_id, &id)
        .await?
        .map(Json)
        .ok_or(AppError::NotFound)
}

async fn create_note(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Json(input): Json<NoteInput>,
) -> AppResult<(StatusCode, Json<repo::Note>)> {
    if input.title.trim().is_empty() {
        return Err(AppError::BadRequest("title is required".into()));
    }
    let note = repo::create(&state.db, &user_id, &input.title, &input.body).await?;
    Ok((StatusCode::CREATED, Json(note)))
}

async fn update_note(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
    Json(input): Json<NoteInput>,
) -> AppResult<Json<repo::Note>> {
    if input.title.trim().is_empty() {
        return Err(AppError::BadRequest("title is required".into()));
    }
    repo::update(&state.db, &user_id, &id, &input.title, &input.body)
        .await?
        .map(Json)
        .ok_or(AppError::NotFound)
}

async fn delete_note(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    if repo::delete(&state.db, &user_id, &id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound)
    }
}
