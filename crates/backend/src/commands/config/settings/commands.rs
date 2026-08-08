pub async fn settings_load_all() -> Result<SettingsLoadResponse, String> {
    tokio::task::spawn_blocking(|| {
        let conn = open_db()?;
        let default_workdir = default_project_workdir()?;
        Ok(SettingsLoadResponse {
            providers: load_providers(&conn)?,
            system: Some(load_system_with_defaults(&conn, &default_workdir)?),
            mcp: load_mcp(&conn)?,
            agents: load_agents(&conn)?,
            ssh: load_ssh(&conn)?,
            memory: load_memory(&conn)?,
            default_workdir,
        })
    })
    .await
    .map_err(|e| format!("settings_load_all join 失败：{e}"))?
}

pub async fn settings_save_providers(payload: Value) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let mut conn = open_db()?;
        save_providers(&mut conn, payload)
    })
    .await
    .map_err(|e| format!("settings_save_providers join 失败：{e}"))?
}

pub async fn settings_save_system(
    payload: Value,
    automation_scheduler: &Arc<AutomationScheduler>) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let mut conn = open_db()?;
        save_system(&mut conn, payload)?;
        // 保存成功后刷新全局代理状态，让 shell env 注入与出网代理即时生效。
        refresh_system_proxy_state(&conn)
    })
    .await
    .map_err(|e| format!("settings_save_system join 失败：{e}"))??;
    // Bash cron tasks execute in the system workdir; reschedule so the new
    // workdir takes effect without an app restart.
    automation_scheduler.request_reload();
    Ok(())
}

pub async fn settings_save_mcp(payload: Value) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let mut conn = open_db()?;
        save_mcp(&mut conn, payload)
    })
    .await
    .map_err(|e| format!("settings_save_mcp join 失败：{e}"))?
}

pub async fn settings_save_memory(payload: Value) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let mut conn = open_db()?;
        save_memory(&mut conn, payload)
    })
    .await
    .map_err(|e| format!("settings_save_memory join 失败：{e}"))?
}

pub async fn settings_save_agents(payload: Value) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let mut conn = open_db()?;
        save_agents(&mut conn, payload)
    })
    .await
    .map_err(|e| format!("settings_save_agents join 失败：{e}"))?
}

/// 从供应商 API 拉模型列表。网络请求收在后端（后端是唯一网络入口），
/// 前端拿到的是供应商原始 JSON（数组或含 data/models 的对象）。
pub async fn provider_models_fetch(
    provider_type: String,
    base_url: String,
    api_key: String,
    use_system_proxy: bool) -> Result<Value, String> {
    let payload = crate::services::provider_models::fetch_provider_models(
        &provider_type,
        &base_url,
        &api_key,
        use_system_proxy,
    )
    .await?;
    serde_json::from_str(&payload).map_err(|e| format!("解析供应商模型列表失败：{e}"))
}

pub async fn settings_apply_ssh_patch(payload: Value) -> Result<SshPatchApplyResponse, String> {
    tokio::task::spawn_blocking(move || {
        let mut conn = open_db()?;
        apply_ssh_patch_with_conn(&mut conn, payload)
    })
    .await
    .map_err(|e| format!("settings_apply_ssh_patch join 失败：{e}"))?
}

pub async fn settings_reset_ssh_known_host(
    host: String,
    port: u16) -> Result<SshKnownHostResetResponse, String> {
    tokio::task::spawn_blocking(move || {
        let deleted = reset_runtime_ssh_known_host(&host, port)?;
        Ok(SshKnownHostResetResponse { deleted })
    })
    .await
    .map_err(|e| format!("settings_reset_ssh_known_host join 失败：{e}"))?
}
