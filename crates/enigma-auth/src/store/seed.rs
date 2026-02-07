use crate::permissions::PERMISSIONS;
use crate::error::AuthError;
use crate::store::AuthStore;

const READ_PERMISSIONS: &[&str] = &[
    "dashboard:read",
    "storage:read",
    "backups:read",
    "buckets:read",
    "cluster:read",
    "tokens:own",
];

const ADMIN_PERMISSIONS: &[&str] = &[
    "dashboard:read",
    "storage:read",
    "storage:write",
    "backups:read",
    "backups:write",
    "buckets:read",
    "buckets:write",
    "cluster:read",
    "cluster:write",
    "users:read",
    "users:write",
    "groups:read",
    "groups:write",
    "tokens:own",
    "tokens:admin",
    "audit:read",
    "settings:read",
    "settings:write",
    "s3:read",
    "s3:write",
    "s3:admin",
];

pub async fn seed_defaults(store: &dyn AuthStore) -> Result<(), AuthError> {
    // Create all permissions
    let mut perm_map = std::collections::HashMap::new();
    for (action, desc) in PERMISSIONS {
        let p = store.create_permission(action, desc).await?;
        perm_map.insert(action.to_string(), p.id);
    }

    // Create wildcard permission for owner
    let wildcard = store.create_permission("*", "All permissions (wildcard)").await?;
    perm_map.insert("*".to_string(), wildcard.id);

    // Create groups (idempotent via name check)
    let groups = [
        ("read", "Read-only access to dashboards and data"),
        ("admin", "Full administrative access"),
        ("owner", "Owner with all permissions"),
    ];

    for (name, desc) in &groups {
        let group = match store.get_group_by_name(name).await {
            Ok(g) => g,
            Err(AuthError::NotFound(_)) => {
                store.create_group(name, desc, true).await?
            }
            Err(e) => return Err(e),
        };

        let perms: &[&str] = match *name {
            "read" => READ_PERMISSIONS,
            "admin" => ADMIN_PERMISSIONS,
            "owner" => &["*"],
            _ => &[],
        };

        for action in perms {
            if let Some(pid) = perm_map.get(*action) {
                store.add_group_permission(&group.id, pid).await?;
            }
        }
    }

    Ok(())
}
