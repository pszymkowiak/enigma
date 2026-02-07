use s3s::dto::*;
use s3s::s3_error;
use s3s::{S3Response, S3Result};
use std::collections::BTreeSet;

use crate::SharedState;

/// Handle ListObjectsV2 with prefix and delimiter support.
pub async fn handle_list_objects_v2(
    state: &SharedState,
    bucket: &str,
    prefix: &str,
    delimiter: &str,
    max_keys: u32,
    start_after: &str,
    continuation_token: &str,
) -> S3Result<S3Response<ListObjectsV2Output>> {
    let db = state.db.lock().map_err(|_| s3_error!(InternalError))?;

    let ns_id = db
        .get_namespace_id(bucket)
        .map_err(|_| s3_error!(InternalError))?
        .ok_or_else(|| s3_error!(NoSuchBucket))?;

    // Use continuation_token as start_after if provided
    let effective_start_after = if !continuation_token.is_empty() {
        continuation_token
    } else {
        start_after
    };

    // Fetch more than max_keys to handle delimiter grouping
    let objects = db
        .list_objects(ns_id, prefix, max_keys + 100, effective_start_after)
        .map_err(|_| s3_error!(InternalError))?;

    let mut contents = Vec::new();
    let mut common_prefixes: BTreeSet<String> = BTreeSet::new();
    let mut count = 0u32;
    let mut is_truncated = false;
    let mut last_key = String::new();

    for (key, size, etag, _created_at) in &objects {
        if count >= max_keys {
            is_truncated = true;
            break;
        }

        // Handle delimiter-based grouping (virtual directories)
        if !delimiter.is_empty() {
            let after_prefix = &key[prefix.len()..];
            if let Some(pos) = after_prefix.find(delimiter) {
                let common_prefix = format!("{}{}{}", prefix, &after_prefix[..pos], delimiter);
                common_prefixes.insert(common_prefix);
                continue;
            }
        }

        contents.push(Object {
            key: Some(key.clone()),
            size: Some(*size as i64),
            e_tag: Some(format!("\"{etag}\"")),
            last_modified: None,
            storage_class: Some(ObjectStorageClass::from_static(
                ObjectStorageClass::STANDARD,
            )),
            owner: None,
            checksum_algorithm: None,
            checksum_type: None,
            restore_status: None,
        });

        last_key = key.clone();
        count += 1;
    }

    let common_prefix_list: Vec<CommonPrefix> = common_prefixes
        .into_iter()
        .map(|p| CommonPrefix { prefix: Some(p) })
        .collect();

    let mut output = ListObjectsV2Output::default();
    output.name = Some(bucket.to_string());
    output.prefix = if prefix.is_empty() {
        None
    } else {
        Some(prefix.to_string())
    };
    output.delimiter = if delimiter.is_empty() {
        None
    } else {
        Some(delimiter.to_string())
    };
    output.max_keys = Some(max_keys as i32);
    output.key_count = Some(contents.len() as i32);
    output.contents = if contents.is_empty() {
        None
    } else {
        Some(contents)
    };
    output.common_prefixes = if common_prefix_list.is_empty() {
        None
    } else {
        Some(common_prefix_list)
    };
    output.is_truncated = Some(is_truncated);
    output.next_continuation_token = if is_truncated { Some(last_key) } else { None };

    Ok(S3Response::new(output))
}
