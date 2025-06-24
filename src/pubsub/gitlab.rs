use anyhow::anyhow;
use gitlab::{
    api::{
        self,
        projects::{self, merge_requests},
        users, AsyncQuery,
    },
    AsyncGitlab, GitlabBuilder,
};
use log::{debug, info};
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt::Debug;
use tokio::sync::OnceCell;

use crate::pubsub::Approval;

static AGENT: OnceCell<Agent> = OnceCell::const_new();

pub fn init(host: String, token: String) -> Result<(), String> {
    AGENT
        .set(Agent::new(host, token))
        .map_err(|err| format!("init gitlab agent: {err}"))
}

pub fn cli() -> &'static Agent {
    AGENT.get().expect("gitlab agent not initialized")
}

pub type GitlabResult<T> = Result<T, GitlabError>;

#[derive(Debug, thiserror::Error)]
pub enum GitlabError {
    #[error("build gitlab client: {0}")]
    Gitlab(#[from] gitlab::GitlabError),

    #[error("build current user: {0}")]
    CurrentUserBuilder(#[from] users::CurrentUserBuilderError),

    #[error("build merge request details: {0}")]
    MergeRequestBuilder(#[from] merge_requests::MergeRequestBuilderError),

    #[error("{msg}: {source}")]
    RestAPI {
        msg: String,
        source: api::ApiError<gitlab::RestError>,
    },

    #[error("build merge request approvals: {0}")]
    MergeRequestApprovalsBuilder(#[from] merge_requests::approvals::MergeRequestApprovalsBuilderError),

    #[error("build approve merge request: {0}")]
    ApproveMergeRequestBuilder(#[from] merge_requests::ApproveMergeRequestBuilderError),

    #[error(transparent)]
    Any(#[from] anyhow::Error),
}

pub struct Agent {
    host: String,
    token: String,
    once: OnceCell<AsyncGitlab>,
}

impl Agent {
    pub async fn current_user(&self) -> GitlabResult<User> {
        debug!("[gitlab] get current user");

        let client = self.get_client().await?;

        let user: User = users::CurrentUser::builder()
            .build()?
            .query_async(client)
            .await
            .map_err(|err| GitlabError::RestAPI {
                msg: "query current user".to_string(),
                source: err,
            })?;

        debug!("[gitlab] current user: {:?}", user.name);

        Ok(user)
    }

    pub async fn get_mr_details(&self, pid: &str, mid: u64) -> GitlabResult<MergeRequestDetails> {
        let client = self.get_client().await?;

        projects::merge_requests::MergeRequest::builder()
            .project(pid)
            .merge_request(mid)
            .build()?
            .query_async(client)
            .await
            .map_err(|err| GitlabError::RestAPI {
                msg: "query merge request details".to_string(),
                source: err,
            })
    }

    // Return back to the caller, and send result to the requester
    pub async fn approve_merge_request(&self, approval: &Approval) -> GitlabResult<()> {
        let (pid, mid) = (&approval.pid, approval.mid);

        // 1. check if you have the right to approve
        self.get_approvals_details(pid, mid).await?.assert_approvable()?;

        // 2. try to approve
        let approvals_info = self.approve_mr(pid, mid).await?.info(pid, mid);
        info!("{}", approvals_info);

        Ok(())
    }

    async fn get_approvals_details(&self, pid: &str, mid: u64) -> GitlabResult<ApprovalsDetails> {
        let client = self.get_client().await?;

        projects::merge_requests::approvals::MergeRequestApprovals::builder()
            .project(pid)
            .merge_request(mid)
            .build()?
            .query_async(client)
            .await
            .map_err(|err| GitlabError::RestAPI {
                msg: "get approval details".to_string(),
                source: err,
            })
    }

    async fn approve_mr(&self, pid: &str, mid: u64) -> GitlabResult<ApprovalResponse> {
        let client = self.get_client().await?;

        projects::merge_requests::ApproveMergeRequest::builder()
            .project(pid)
            .merge_request(mid)
            .build()?
            .query_async(client)
            .await
            .map_err(|err| GitlabError::RestAPI {
                msg: "approve merge request".to_string(),
                source: err,
            })
    }

    fn new(host: String, token: String) -> Self {
        Self {
            host,
            token,
            once: OnceCell::const_new(),
        }
    }

    async fn get_client(&self) -> GitlabResult<&AsyncGitlab> {
        let client = self
            .once
            .get_or_try_init(|| async {
                GitlabBuilder::new(self.host.clone(), self.token.clone())
                    .build_async()
                    .await
            })
            .await?;

        Ok(client)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: u64,          // unique id
    pub name: String,     // nickname
    pub username: String, // the unique name for key: $username:$id
}

// gitlab approvals details
#[derive(Debug, Deserialize)]
pub struct ApprovalsDetails {
    state: String, // like 'merged'
    user_can_aprove: bool,
    user_has_approved: bool,
}

impl ApprovalsDetails {
    fn assert_approvable(&self) -> GitlabResult<bool> {
        if matches!(self.state.as_str(), "merged") {
            return Err(GitlabError::from(anyhow!("already merged")));
        }
        if self.user_has_approved {
            return Err(GitlabError::from(anyhow!("already approved")));
        }
        if !self.user_can_aprove {
            return Err(GitlabError::from(anyhow!(
                "you can't approve, please contact the requester"
            )));
        }

        Ok(true)
    }
}

// This is not the mr details reqested from gitlab,
// it's a summary of the merge request sent by the requester via pubsub.
#[derive(Debug, Deserialize)]
pub struct MergeRequestDetails {
    author: User,
    title: String,
    source_branch: String,
    target_branch: String,
}

impl MergeRequestDetails {
    pub fn info(&self, pid: &str) -> String {
        format!(
            "{} ({})\n\t[{}] requests to merge <{}> into <{}>",
            pid, self.title, self.author.name, self.source_branch, self.target_branch,
        )
    }
}

// gitlab approval response of a merge request
#[derive(Debug, Deserialize)]
struct ApprovalResponse {
    state: String,
    #[serde(deserialize_with = "deserialize_approved_by")]
    approved_by: Vec<User>,
}

impl ApprovalResponse {
    fn info(&self, pid: &str, mid: u64) -> String {
        let approvers = self
            .approved_by
            .iter()
            .map(|user| user.name.as_str())
            .collect::<Vec<&str>>()
            .join(", ");

        format!("{}/{}, {}, approvers: ({})", pid, mid, self.state, approvers)
    }
}

#[derive(Deserialize)]
struct UserWrapper {
    user: User,
}

fn deserialize_approved_by<'de, D>(deserializer: D) -> Result<Vec<User>, D::Error>
where
    D: Deserializer<'de>,
{
    let wrapped: Vec<UserWrapper> = Vec::deserialize(deserializer)?;

    Ok(wrapped.into_iter().map(|w| w.user).collect())
}
