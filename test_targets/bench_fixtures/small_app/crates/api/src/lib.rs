use std::{collections::HashMap, sync::Arc};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
};
use serde::{Deserialize, Serialize};
use small_app_domain::{
    CreateWorkspace, Plan, UsageWindow, Workspace, WorkspaceId, WorkspaceSummary,
};
use tokio::sync::RwLock;
use tracing::instrument;

#[derive(Debug, Clone, Default)]
pub struct AppState {
    workspaces: Arc<RwLock<HashMap<String, Workspace>>>,
}

impl AppState {
    pub async fn insert(&self, workspace: Workspace) {
        self.workspaces
            .write()
            .await
            .insert(workspace.id.as_str().to_owned(), workspace);
    }

    pub async fn summaries(&self, filter: WorkspaceFilter) -> Vec<WorkspaceSummary> {
        let workspaces = self.workspaces.read().await;
        let mut summaries = workspaces
            .values()
            .filter(|workspace| filter.matches(workspace))
            .map(Workspace::summary)
            .collect::<Vec<_>>();
        summaries.sort_by(|left, right| left.name.cmp(&right.name));
        summaries
    }

    pub async fn find(&self, id: &WorkspaceId) -> Option<WorkspaceSummary> {
        self.workspaces
            .read()
            .await
            .get(id.as_str())
            .map(Workspace::summary)
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/workspaces", get(list_workspaces).post(create_workspace))
        .route("/workspaces/{id}", get(get_workspace))
        .with_state(state)
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { ok: true })
}

#[instrument(skip(state))]
async fn list_workspaces(
    State(state): State<AppState>,
    Query(filter): Query<WorkspaceFilter>,
) -> Json<Vec<WorkspaceSummary>> {
    Json(state.summaries(filter).await)
}

#[instrument(skip(state, request))]
async fn create_workspace(
    State(state): State<AppState>,
    Json(request): Json<CreateWorkspace>,
) -> Result<(StatusCode, Json<WorkspaceSummary>), (StatusCode, Json<ApiError>)> {
    let workspace = Workspace::from_request(request).map_err(ApiError::from_validation)?;
    let summary = workspace.summary();
    state.insert(workspace).await;

    Ok((StatusCode::CREATED, Json(summary)))
}

#[instrument(skip(state))]
async fn get_workspace(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<WorkspaceSummary>, (StatusCode, Json<ApiError>)> {
    let id = WorkspaceId::new(id);
    state
        .find(&id)
        .await
        .map(Json)
        .ok_or_else(|| ApiError::not_found(id.as_str()))
}

pub async fn seed_state() -> AppState {
    let state = AppState::default();
    let samples = [
        CreateWorkspace {
            slug: "acme".to_owned(),
            name: "Acme Research".to_owned(),
            owner_email: "ops@acme.example".to_owned(),
            plan: Plan::Team,
        },
        CreateWorkspace {
            slug: "northwind".to_owned(),
            name: "Northwind Labs".to_owned(),
            owner_email: "admin@northwind.example".to_owned(),
            plan: Plan::Enterprise,
        },
    ];

    for request in samples {
        let mut workspace =
            Workspace::from_request(request).expect("sample workspaces should be valid");
        workspace.usage = UsageWindow {
            active_users: 9,
            requests: 42_000,
            storage_gib: 18,
        };
        state.insert(workspace).await;
    }

    state
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct WorkspaceFilter {
    pub plan: Option<Plan>,
    pub min_utilization: Option<u32>,
}

impl WorkspaceFilter {
    fn matches(self, workspace: &Workspace) -> bool {
        if let Some(plan) = self.plan
            && workspace.plan != plan
        {
            return false;
        }

        if let Some(min_utilization) = self.min_utilization
            && workspace.usage.utilization_score(workspace.plan) < min_utilization
        {
            return false;
        }

        true
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
struct HealthResponse {
    ok: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiError {
    code: &'static str,
    message: String,
}

impl ApiError {
    fn from_validation(error: small_app_domain::ValidationError) -> (StatusCode, Json<Self>) {
        (
            StatusCode::BAD_REQUEST,
            Json(Self {
                code: "validation_failed",
                message: error.to_string(),
            }),
        )
    }

    fn not_found(id: &str) -> (StatusCode, Json<Self>) {
        (
            StatusCode::NOT_FOUND,
            Json(Self {
                code: "workspace_not_found",
                message: format!("workspace '{id}' was not found"),
            }),
        )
    }
}
