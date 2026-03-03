use serde::{Deserialize, Serialize};
use tauri::command;

#[derive(Debug, Serialize, Deserialize)]
struct YesCaptchaCreateTaskRequest {
    #[serde(rename = "clientKey")]
    client_key: String,
    task: YesCaptchaTask,
}

#[derive(Debug, Serialize, Deserialize)]
struct YesCaptchaTask {
    #[serde(rename = "type")]
    task_type: String,
    #[serde(rename = "websiteURL")]
    website_url: String,
    #[serde(rename = "websiteKey")]
    website_key: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct YesCaptchaCreateTaskResponse {
    #[serde(rename = "errorId")]
    error_id: i32,
    #[serde(rename = "errorCode")]
    error_code: Option<String>,
    #[serde(rename = "errorDescription")]
    error_description: Option<String>,
    #[serde(rename = "taskId")]
    task_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct YesCaptchaGetResultRequest {
    #[serde(rename = "clientKey")]
    client_key: String,
    #[serde(rename = "taskId")]
    task_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct YesCaptchaGetResultResponse {
    #[serde(rename = "errorId")]
    error_id: i32,
    #[serde(rename = "errorCode")]
    error_code: Option<String>,
    #[serde(rename = "errorDescription")]
    error_description: Option<String>,
    status: Option<String>,
    solution: Option<YesCaptchaSolution>,
}

#[derive(Debug, Serialize, Deserialize)]
struct YesCaptchaSolution {
    token: String,
}

#[command]
pub async fn solve_turnstile_with_yescaptcha(
    api_key: String,
    sitekey: String,
    page_url: String,
    proxy_url: Option<String>,
    api_endpoint: Option<String>,
) -> Result<String, String> {
    let mut client_builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .connect_timeout(std::time::Duration::from_secs(30))
        .danger_accept_invalid_certs(false);

    // 如果提供了代理，配置代理
    if let Some(proxy) = proxy_url {
        if !proxy.is_empty() {
            println!("[YesCaptcha] 使用代理: {}", proxy);
            let proxy_obj = reqwest::Proxy::all(&proxy)
                .map_err(|e| format!("配置代理失败: {}", e))?;
            client_builder = client_builder.proxy(proxy_obj);
        }
    }

    let client = client_builder
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

    // 使用自定义端点或默认官方端点
    let endpoints = if let Some(custom_endpoint) = api_endpoint {
        if !custom_endpoint.is_empty() {
            println!("[YesCaptcha] 使用自定义端点: {}", custom_endpoint);
            vec![custom_endpoint]
        } else {
            vec!["https://api.yescaptcha.com".to_string()]
        }
    } else {
        vec!["https://api.yescaptcha.com".to_string()]
    };

    let mut last_error = String::new();

    for endpoint in endpoints {
        match solve_with_endpoint(&client, &endpoint, &api_key, &sitekey, &page_url).await {
            Ok(token) => return Ok(token),
            Err(e) => {
                last_error = e;
                eprintln!("[YesCaptcha] 端点 {} 失败: {}", endpoint, last_error);
                continue;
            }
        }
    }

    Err(format!("所有 YesCaptcha 端点均失败: {}", last_error))
}

async fn solve_with_endpoint(
    client: &reqwest::Client,
    endpoint: &str,
    api_key: &str,
    sitekey: &str,
    page_url: &str,
) -> Result<String, String> {
    println!("[YesCaptcha] 尝试使用端点: {}", endpoint);
    println!("[YesCaptcha] 参数 - sitekey: {}, pageUrl: {}", sitekey, page_url);

    // 1. 创建任务
    let create_url = format!("{}/createTask", endpoint);
    let create_request = YesCaptchaCreateTaskRequest {
        client_key: api_key.to_string(),
        task: YesCaptchaTask {
            task_type: "TurnstileTaskProxyless".to_string(),
            website_url: page_url.to_string(),
            website_key: sitekey.to_string(),
        },
    };
    
    println!("[YesCaptcha] 请求数据: {:?}", create_request);

    let create_response = client
        .post(&create_url)
        .json(&create_request)
        .send()
        .await
        .map_err(|e| format!("创建任务请求失败: {}", e))?;

    let create_result: YesCaptchaCreateTaskResponse = create_response
        .json()
        .await
        .map_err(|e| format!("解析创建任务响应失败: {}", e))?;

    // 检查是否有错误
    if create_result.error_id != 0 {
        return Err(format!(
            "创建任务失败 (errorId: {}): {}",
            create_result.error_id,
            create_result
                .error_description
                .or(create_result.error_code)
                .unwrap_or_else(|| "未知错误".to_string())
        ));
    }

    let task_id = create_result
        .task_id
        .ok_or_else(|| "未返回任务ID".to_string())?;

    println!("[YesCaptcha] 任务已创建，ID: {}", task_id);

    // 2. 轮询获取结果（按照官方示例，每3秒检查一次，最多120次）
    let max_attempts = 120;
    let result_url = format!("{}/getTaskResult", endpoint);

    for i in 0..max_attempts {
        // 等待3秒（官方示例的间隔）
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

        let result_request = YesCaptchaGetResultRequest {
            client_key: api_key.to_string(),
            task_id: task_id.clone(),
        };

        let result_response = client
            .post(&result_url)
            .json(&result_request)
            .send()
            .await
            .map_err(|e| format!("获取结果请求失败: {}", e))?;

        let result: YesCaptchaGetResultResponse = result_response
            .json()
            .await
            .map_err(|e| format!("解析获取结果响应失败: {}", e))?;

        // 检查是否有错误
        if result.error_id != 0 {
            return Err(format!(
                "获取结果失败 (errorId: {}): {}",
                result.error_id,
                result
                    .error_description
                    .or(result.error_code)
                    .unwrap_or_else(|| "未知错误".to_string())
            ));
        }

        // 检查是否有 solution
        if let Some(solution) = result.solution {
            println!("[YesCaptcha] 验证成功！");
            return Ok(solution.token);
        }

        // 如果没有 solution，继续等待
        println!("[YesCaptcha] 等待验证完成... ({}/{})", i + 1, max_attempts);
    }

    Err("验证超时（360秒）".to_string())
}
