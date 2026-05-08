use serde::{Deserialize, Serialize};
use tracing::instrument;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkspaceId(String);

impl WorkspaceId {
    pub fn new(raw: impl Into<String>) -> Self {
        let raw = raw.into();
        assert!(!raw.trim().is_empty(), "workspace id should not be empty");
        Self(raw)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Plan {
    Free,
    Team,
    Enterprise,
}

impl Plan {
    pub fn included_seats(self) -> u32 {
        match self {
            Self::Free => 3,
            Self::Team => 12,
            Self::Enterprise => 100,
        }
    }

    pub fn monthly_base_price(self) -> u32 {
        match self {
            Self::Free => 0,
            Self::Team => 49,
            Self::Enterprise => 499,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureFlags {
    pub audit_log: bool,
    pub api_access: bool,
    pub priority_support: bool,
}

impl FeatureFlags {
    pub fn for_plan(plan: Plan) -> Self {
        match plan {
            Plan::Free => Self {
                audit_log: false,
                api_access: false,
                priority_support: false,
            },
            Plan::Team => Self {
                audit_log: true,
                api_access: true,
                priority_support: false,
            },
            Plan::Enterprise => Self {
                audit_log: true,
                api_access: true,
                priority_support: true,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageWindow {
    pub active_users: u32,
    pub requests: u64,
    pub storage_gib: u32,
}

impl UsageWindow {
    pub fn utilization_score(self, plan: Plan) -> u32 {
        let seat_pressure = self.active_users.saturating_mul(100) / plan.included_seats();
        let request_pressure = (self.requests / 1_000).min(100) as u32;
        let storage_pressure = self.storage_gib.min(100);

        seat_pressure.max(request_pressure).max(storage_pressure)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub name: String,
    pub owner_email: String,
    pub plan: Plan,
    pub flags: FeatureFlags,
    pub usage: UsageWindow,
}

impl Workspace {
    pub fn from_request(request: CreateWorkspace) -> Result<Self, ValidationError> {
        request.validate()?;

        Ok(Self {
            id: WorkspaceId::new(request.slug),
            name: request.name,
            owner_email: request.owner_email,
            plan: request.plan,
            flags: FeatureFlags::for_plan(request.plan),
            usage: UsageWindow {
                active_users: 1,
                requests: 0,
                storage_gib: 0,
            },
        })
    }

    #[instrument(skip(self))]
    pub fn summary(&self) -> WorkspaceSummary {
        WorkspaceSummary {
            id: self.id.as_str().to_owned(),
            name: self.name.clone(),
            plan: self.plan,
            monthly_price: self.plan.monthly_base_price(),
            utilization_score: self.usage.utilization_score(self.plan),
            requires_sales_contact: self.plan == Plan::Enterprise && self.usage.active_users > 80,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateWorkspace {
    pub slug: String,
    pub name: String,
    pub owner_email: String,
    pub plan: Plan,
}

impl CreateWorkspace {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.slug.trim().is_empty() {
            return Err(ValidationError::MissingField("slug"));
        }

        if !self
            .slug
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        {
            return Err(ValidationError::InvalidSlug);
        }

        if self.name.trim().is_empty() {
            return Err(ValidationError::MissingField("name"));
        }

        if !self.owner_email.contains('@') {
            return Err(ValidationError::InvalidOwnerEmail);
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSummary {
    pub id: String,
    pub name: String,
    pub plan: Plan,
    pub monthly_price: u32,
    pub utilization_score: u32,
    pub requires_sales_contact: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationError {
    MissingField(&'static str),
    InvalidSlug,
    InvalidOwnerEmail,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingField(field) => write!(f, "missing required field: {field}"),
            Self::InvalidSlug => f.write_str("slug must use ASCII letters, digits, or '-'"),
            Self::InvalidOwnerEmail => f.write_str("owner email should contain '@'"),
        }
    }
}

impl std::error::Error for ValidationError {}
