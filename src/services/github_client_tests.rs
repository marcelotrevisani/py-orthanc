#[cfg(test)]
mod tests {
    use crate::models::github::{WorkflowConclusion, WorkflowStatus};
    use crate::services::github_client::GitHubClient;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn setup_client() -> (MockServer, GitHubClient) {
        let mock_server = MockServer::start().await;
        let client = GitHubClient::new("test-token".to_string(), mock_server.uri()).unwrap();
        (mock_server, client)
    }

    #[test]
    fn new_rejects_invalid_base_url() {
        let err = GitHubClient::new("t".to_string(), "http://bad host".to_string())
            .err()
            .unwrap();
        assert!(err.to_string().contains("invalid GitHub API base URL"));
    }

    #[tokio::test]
    async fn test_list_workflow_runs_success() {
        let (mock_server, client) = setup_client().await;

        let response_body = serde_json::json!({
            "total_count": 1,
            "workflow_runs": [
                {
                    "id": 123,
                    "name": "CI",
                    "status": "completed",
                    "conclusion": "success",
                    "html_url": "https://github.com/owner/repo/actions/runs/123",
                    "head_branch": "main",
                    "created_at": "2024-01-15T10:00:00Z",
                    "updated_at": "2024-01-15T10:05:00Z"
                }
            ]
        });

        Mock::given(method("GET"))
            .and(path("/repos/owner/repo/actions/runs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&mock_server)
            .await;

        let runs = client
            .list_workflow_runs("owner", "repo", None)
            .await
            .unwrap();

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].id, 123);
        assert_eq!(runs[0].name, "CI");
        assert_eq!(runs[0].status, WorkflowStatus::Completed);
        assert_eq!(runs[0].conclusion, Some(WorkflowConclusion::Success));
        assert_eq!(runs[0].head_branch, "main");
    }

    #[tokio::test]
    async fn test_list_workflow_runs_with_branch_filter() {
        let (mock_server, client) = setup_client().await;

        let response_body = serde_json::json!({
            "total_count": 0,
            "workflow_runs": []
        });

        Mock::given(method("GET"))
            .and(path("/repos/owner/repo/actions/runs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&mock_server)
            .await;

        let runs = client
            .list_workflow_runs("owner", "repo", Some("feature-branch"))
            .await
            .unwrap();

        assert_eq!(runs.len(), 0);
    }

    #[tokio::test]
    async fn test_rerun_workflow() {
        let (mock_server, client) = setup_client().await;

        Mock::given(method("POST"))
            .and(path("/repos/owner/repo/actions/runs/123/rerun"))
            .respond_with(ResponseTemplate::new(201))
            .mount(&mock_server)
            .await;

        let result = client.rerun_workflow("owner", "repo", 123).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_github_api_401_error() {
        let (mock_server, client) = setup_client().await;

        Mock::given(method("GET"))
            .and(path("/repos/owner/repo/actions/runs"))
            .respond_with(
                ResponseTemplate::new(401)
                    .set_body_json(serde_json::json!({"message": "Bad credentials"})),
            )
            .mount(&mock_server)
            .await;

        let result = client.list_workflow_runs("owner", "repo", None).await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        let err_str = err.to_string();
        assert!(err_str.contains("401") || err_str.contains("Bad credentials"));
    }

    #[tokio::test]
    async fn test_github_api_404_error() {
        let (mock_server, client) = setup_client().await;

        Mock::given(method("GET"))
            .and(path("/repos/owner/nonexistent/actions/runs"))
            .respond_with(
                ResponseTemplate::new(404)
                    .set_body_json(serde_json::json!({"message": "Not Found"})),
            )
            .mount(&mock_server)
            .await;

        let result = client
            .list_workflow_runs("owner", "nonexistent", None)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_workflow_status_parsing() {
        let (mock_server, client) = setup_client().await;

        let response_body = serde_json::json!({
            "total_count": 3,
            "workflow_runs": [
                {
                    "id": 1, "name": "A", "status": "queued", "conclusion": null,
                    "html_url": "https://x", "head_branch": "main",
                    "created_at": "2024-01-15T10:00:00Z", "updated_at": "2024-01-15T10:00:00Z"
                },
                {
                    "id": 2, "name": "B", "status": "in_progress", "conclusion": null,
                    "html_url": "https://x", "head_branch": "main",
                    "created_at": "2024-01-15T10:00:00Z", "updated_at": "2024-01-15T10:00:00Z"
                },
                {
                    "id": 3, "name": "C", "status": "completed", "conclusion": "cancelled",
                    "html_url": "https://x", "head_branch": "main",
                    "created_at": "2024-01-15T10:00:00Z", "updated_at": "2024-01-15T10:00:00Z"
                }
            ]
        });

        Mock::given(method("GET"))
            .and(path("/repos/o/r/actions/runs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&mock_server)
            .await;

        let runs = client.list_workflow_runs("o", "r", None).await.unwrap();

        assert_eq!(runs[0].status, WorkflowStatus::Queued);
        assert_eq!(runs[0].conclusion, None);
        assert_eq!(runs[1].status, WorkflowStatus::InProgress);
        assert_eq!(runs[2].conclusion, Some(WorkflowConclusion::Cancelled));
    }
}
