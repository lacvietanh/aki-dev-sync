use crate::agent_usage::probe_result::AgentUsageResponse;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

/// Parses raw framed output from `get-antigravity-usage.sh` and assembles the unified AG snapshot payload.
pub(crate) fn parse_antigravity_frames(raw_output: &str, now: i64) -> Result<Option<AgentUsageResponse>, String> {
    let raw_output = raw_output.trim();
    if raw_output.is_empty() {
        return Ok(None);
    }

    let chunks: Vec<&str> = raw_output.split("|||AGPROC|||").collect();
    let mut snapshots: Vec<Value> = Vec::new();
    let mut seen_instance_keys: HashSet<String> = HashSet::new();

    for chunk in chunks {
        let chunk = chunk.trim();
        if chunk.is_empty() {
            continue;
        }

        let proc_info = parse_frame_chunk(chunk);
        if proc_info.status_code < 200 || proc_info.status_code >= 300 {
            continue;
        }

        let raw_status: Value = match serde_json::from_str(&proc_info.status_body) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let user_status = raw_status.get("userStatus").unwrap_or(&raw_status);
        let email = match user_status.get("email").and_then(|e| e.as_str()) {
            Some(e) if !e.trim().is_empty() => e.trim(),
            _ => continue,
        };

        let raw_summary: Option<Value> = if !proc_info.summary_body.trim().is_empty() {
            serde_json::from_str(&proc_info.summary_body).ok()
        } else {
            None
        };

        let quota_summary = raw_summary.as_ref().and_then(|s| {
            s.get("response").cloned().or_else(|| Some(s.clone()))
        });

        let mut snapshot = build_local_quota_snapshot(user_status, quota_summary, now);
        let source_type = if proc_info.proc_type.is_empty() {
            "ide"
        } else {
            &proc_info.proc_type
        };
        if let Some(obj) = snapshot.as_object_mut() {
            obj.insert("sourceType".to_string(), json!(source_type));
        }

        let instance_key = format!("{}:{}", email, source_type);
        if !seen_instance_keys.contains(&instance_key) {
            seen_instance_keys.insert(instance_key);
            snapshots.push(snapshot);
        }
    }

    if snapshots.is_empty() {
        return Ok(None);
    }

    // Collapse shared desktop + cli sessions on same email (P36)
    let mut final_snapshots: Vec<Value> = Vec::new();
    let mut core_map: HashMap<String, usize> = HashMap::new();

    for s in snapshots {
        let source_type = s.get("sourceType").and_then(|v| v.as_str()).unwrap_or("ide").to_string();
        let email = s.get("email").and_then(|v| v.as_str()).unwrap_or("").to_string();

        if source_type == "desktop" || source_type == "cli" {
            if let Some(&idx) = core_map.get(&email) {
                if let Some(existing_obj) = final_snapshots[idx].as_object_mut() {
                    existing_obj.insert("sourceType".to_string(), json!("desktop_cli"));
                }
            } else {
                core_map.insert(email, final_snapshots.len());
                final_snapshots.push(s);
            }
        } else {
            final_snapshots.push(s);
        }
    }

    if final_snapshots.is_empty() {
        return Ok(None);
    }

    let mut primary = final_snapshots[0].clone();
    if final_snapshots.len() > 1 {
        let all_accounts: Vec<Value> = final_snapshots
            .iter()
            .map(|s| {
                json!({
                    "email": s.get("email"),
                    "method": s.get("method"),
                    "sourceType": s.get("sourceType"),
                    "userTier": s.get("userTier"),
                    "quotaSummary": s.get("quotaSummary"),
                    "models": s.get("models"),
                    "timestamp": s.get("timestamp")
                })
            })
            .collect();

        if let Some(obj) = primary.as_object_mut() {
            obj.insert("allAccounts".to_string(), Value::Array(all_accounts));
        }
    }

    let content = serde_json::to_string(&primary).map_err(|e| e.to_string())?;
    let now_str = now.to_string();
    Ok(Some(AgentUsageResponse {
        content,
        fetched_at: now_str.clone(),
        file_modified_at: now_str,
    }))
}

struct ParsedFrameChunk {
    proc_type: String,
    status_code: i64,
    status_body: String,
    summary_body: String,
}

fn parse_frame_chunk(chunk: &str) -> ParsedFrameChunk {
    let mut proc_type = String::new();
    let mut status_code = 0i64;
    let mut status_body = String::new();
    let mut summary_body = String::new();

    let mut current_tag = "";
    let mut current_buf = String::new();

    for line in chunk.lines() {
        if let Some(val) = line.strip_prefix("|||TYPE|||") {
            proc_type = val.trim().to_string();
            current_tag = "";
        } else if let Some(val) = line.strip_prefix("|||STATUSCODE|||") {
            status_code = val.trim().parse::<i64>().unwrap_or(0);
            current_tag = "";
        } else if let Some(val) = line.strip_prefix("|||STATUS|||") {
            current_tag = "STATUS";
            current_buf = val.to_string();
        } else if let Some(val) = line.strip_prefix("|||SUMMARYCODE|||") {
            if current_tag == "STATUS" {
                status_body = current_buf.clone();
            }
            current_tag = "";
        } else if let Some(val) = line.strip_prefix("|||SUMMARY|||") {
            current_tag = "SUMMARY";
            current_buf = val.to_string();
        } else {
            if !current_tag.is_empty() {
                if !current_buf.is_empty() {
                    current_buf.push('\n');
                }
                current_buf.push_str(line);
            }
        }
    }

    if current_tag == "STATUS" {
        status_body = current_buf;
    } else if current_tag == "SUMMARY" {
        summary_body = current_buf;
    }

    ParsedFrameChunk {
        proc_type,
        status_code,
        status_body,
        summary_body,
    }
}

fn build_local_quota_snapshot(user_status: &Value, quota_summary: Option<Value>, now: i64) -> Value {
    let email = user_status.get("email").and_then(|v| v.as_str()).unwrap_or("");
    let user_tier = user_status.get("userTier").cloned().unwrap_or(Value::Null);

    let extracted_quota_models = extract_quota_models(user_status, now);
    let parsed_models: Vec<Value> = extracted_quota_models
        .iter()
        .map(|m| parse_model_quota(m, now))
        .collect();

    let iso_now = format_iso_timestamp(now);

    json!({
        "timestamp": iso_now,
        "method": "local",
        "email": email,
        "userTier": user_tier,
        "models": parsed_models,
        "quotaSummary": quota_summary.unwrap_or(Value::Null)
    })
}

fn extract_quota_models(data: &Value, now: i64) -> Vec<Value> {
    let cascade_data = data.get("cascadeModelConfigData");
    let client_model_configs = cascade_data.and_then(|c| c.get("clientModelConfigs")).and_then(|v| v.as_array());

    let mut models = Vec::new();
    if let Some(configs) = client_model_configs {
        for m in configs {
            let model_or_alias = m.get("modelOrAlias");
            let model_id = model_or_alias
                .and_then(|ma| ma.get("model"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            let label = m.get("label").and_then(|v| v.as_str());
            let quota_info = m.get("quotaInfo");
            let remaining_fraction = quota_info.and_then(|q| q.get("remainingFraction")).and_then(|v| v.as_f64());
            let reset_time = quota_info.and_then(|q| q.get("resetTime")).and_then(|v| v.as_str());

            let used_percentage = remaining_fraction.map(|rf| 1.0 - rf);
            let time_until_reset_ms = reset_time.and_then(|rt| parse_iso_reset_time(rt, now));

            let is_exhausted = remaining_fraction.map(|rf| rf == 0.0).unwrap_or(false);

            models.push(json!({
                "modelId": model_id,
                "displayName": label,
                "label": label,
                "quota": {
                    "remaining": Value::Null,
                    "limit": Value::Null,
                    "usedPercentage": used_percentage,
                    "remainingPercentage": remaining_fraction,
                    "resetTime": reset_time,
                    "timeUntilResetMs": time_until_reset_ms
                },
                "isExhausted": is_exhausted
            }));
        }
    }
    models
}

fn parse_model_quota(model: &Value, now: i64) -> Value {
    let model_id = model.get("modelId").and_then(|v| v.as_str()).unwrap_or("unknown");
    let label_str = model.get("label").and_then(|v| v.as_str());
    let display_str = model.get("displayName").and_then(|v| v.as_str());

    let label = label_str
        .or(display_str)
        .unwrap_or(model_id);

    let quota = model.get("quota");
    let remaining_percentage = quota.and_then(|q| q.get("remainingPercentage")).and_then(|v| v.as_f64());
    let reset_time = quota.and_then(|q| q.get("resetTime")).and_then(|v| v.as_str());
    let time_until_reset_ms = reset_time.and_then(|rt| parse_iso_reset_time(rt, now));

    let is_exhausted = model.get("isExhausted")
        .and_then(|v| v.as_bool())
        .unwrap_or_else(|| remaining_percentage.map(|r| r == 0.0).unwrap_or(false));

    let is_autocomplete_only = model_id.contains("gemini-2.5")
        || label.contains("Gemini 2.5")
        || display_str.map(|d| d.contains("Gemini 2.5")).unwrap_or(false);

    json!({
        "label": label,
        "modelId": model_id,
        "remainingPercentage": remaining_percentage,
        "isExhausted": is_exhausted,
        "resetTime": reset_time,
        "timeUntilResetMs": time_until_reset_ms,
        "isAutocompleteOnly": is_autocomplete_only
    })
}

fn parse_iso_reset_time(s: &str, now_secs: i64) -> Option<i64> {
    if s.len() < 19 {
        return None;
    }
    let year: i64 = s.get(0..4)?.parse().ok()?;
    let month: i64 = s.get(5..7)?.parse().ok()?;
    let day: i64 = s.get(8..10)?.parse().ok()?;
    let hour: i64 = s.get(11..13)?.parse().ok()?;
    let min: i64 = s.get(14..16)?.parse().ok()?;
    let sec: i64 = s.get(17..19)?.parse().ok()?;

    let days = days_from_civil(year, month, day);
    let target_secs = days * 86400 + hour * 3600 + min * 60 + sec;
    let diff_ms = (target_secs - now_secs) * 1000;
    if diff_ms > 0 {
        Some(diff_ms)
    } else {
        None
    }
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = y - if m <= 2 { 1 } else { 0 };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn format_iso_timestamp(now_secs: i64) -> String {
    // Simple UTC ISO timestamp string formatting
    let (year, month, day, hour, min, sec) = civil_from_days(now_secs / 86400);
    let rem = now_secs % 86400;
    let rem = if rem < 0 { rem + 86400 } else { rem };
    let hour = rem / 3600;
    let min = (rem % 3600) / 60;
    let sec = rem % 60;
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.000Z", year, month, day, hour, min, sec)
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let y = y + if m <= 2 { 1 } else { 0 };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_antigravity_frames_single() {
        let raw = r#"|||AGPROC|||1234
|||TYPE|||ide
|||STATUSCODE|||200
|||STATUS|||{"userStatus":{"email":"user@example.com","userTier":"PRO","cascadeModelConfigData":{"clientModelConfigs":[{"modelOrAlias":{"model":"gemini-2.5-flash"},"label":"Gemini 2.5 Flash","quotaInfo":{"remainingFraction":0.8,"resetTime":"2026-07-29T18:00:00Z"}}]}}}
|||SUMMARYCODE|||200
|||SUMMARY|||{"response":{"groups":[]}}
"#;
        let res = parse_antigravity_frames(raw, 1750000000).unwrap();
        assert!(res.is_some());
        let payload = res.unwrap();
        assert!(payload.content.contains("user@example.com"));
        assert!(payload.content.contains("gemini-2.5-flash"));
    }
}
