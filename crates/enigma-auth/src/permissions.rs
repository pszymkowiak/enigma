pub const PERMISSIONS: &[(&str, &str)] = &[
    ("dashboard:read", "View dashboard and system status"),
    ("storage:read", "View storage providers and chunk stats"),
    ("storage:write", "Modify storage configuration"),
    ("backups:read", "View backup history"),
    ("backups:write", "Create and manage backups"),
    ("buckets:read", "List S3 buckets and objects"),
    ("buckets:write", "Create and delete S3 buckets"),
    ("cluster:read", "View cluster status"),
    ("cluster:write", "Manage cluster nodes"),
    ("users:read", "View user list"),
    ("users:write", "Create, update, and delete users"),
    ("groups:read", "View group list"),
    ("groups:write", "Create, update, and delete groups"),
    ("tokens:own", "Manage own API tokens"),
    ("tokens:admin", "Manage all API tokens"),
    ("audit:read", "View audit logs"),
    ("settings:read", "View system settings"),
    ("settings:write", "Modify system settings"),
    ("s3:read", "S3 read operations"),
    ("s3:write", "S3 write operations"),
    ("s3:admin", "S3 administrative operations"),
];

pub fn has_permission(user_permissions: &[String], required: &str) -> bool {
    user_permissions.iter().any(|p| p == "*" || p == required)
}
