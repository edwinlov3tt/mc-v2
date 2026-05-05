//! CLI command handler functions for schedule management.
//!
//! - `schedule_add`: validate cron, generate ID, persist to registry.
//! - `schedule_list`: return all schedules.
//! - `schedule_remove`: remove a schedule by ID.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::TesseraError;

use super::cron_expr::{now_rfc3339, CronExpr};
use super::registry::{Schedule, ScheduleRegistry};

/// Add a new schedule. Validates the cron expression, generates a unique ID,
/// and persists to the registry.
///
/// Returns the generated schedule ID.
pub fn schedule_add(
    model_dir: &Path,
    recipe_path: &str,
    cron_expr: &str,
) -> Result<String, TesseraError> {
    // Validate cron expression.
    CronExpr::parse(cron_expr)?;

    let mut registry = ScheduleRegistry::load(model_dir)?;

    let id = generate_schedule_id(recipe_path);
    let now = now_rfc3339();

    let schedule = Schedule {
        id: id.clone(),
        recipe_path: recipe_path.to_string(),
        cron: cron_expr.to_string(),
        created_at: now,
        status: "active".to_string(),
        last_run: None,
        last_result: None,
        failure_count: 0,
        next_retry: None,
    };

    registry.schedules.push(schedule);
    registry.save(model_dir)?;

    Ok(id)
}

/// List all schedules in the registry.
pub fn schedule_list(model_dir: &Path) -> Result<Vec<Schedule>, TesseraError> {
    let registry = ScheduleRegistry::load(model_dir)?;
    Ok(registry.schedules)
}

/// Remove a schedule by ID. Returns an error if the schedule doesn't exist.
pub fn schedule_remove(model_dir: &Path, schedule_id: &str) -> Result<(), TesseraError> {
    let mut registry = ScheduleRegistry::load(model_dir)?;

    let len_before = registry.schedules.len();
    registry.schedules.retain(|s| s.id != schedule_id);

    if registry.schedules.len() == len_before {
        return Err(TesseraError::SidecarInconsistent {
            message: format!("schedule {schedule_id:?} not found in registry"),
        });
    }

    registry.save(model_dir)?;
    Ok(())
}

/// Generate a schedule ID: `sched_<recipe_name_safe>_<unix_secs>_<nanos_low>`.
fn generate_schedule_id(recipe_path: &str) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let nanos_low = now.subsec_nanos();

    // Extract recipe name (without extension and path).
    let name = Path::new(recipe_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");

    // Sanitize: replace non-alphanumeric with underscore, truncate.
    let safe_name: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(32)
        .collect();

    format!("sched_{safe_name}_{secs}_{nanos_low}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn add_list_remove_cycle() {
        let dir = std::env::temp_dir().join("mc_tessera_schedule_cmd_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(".tessera")).unwrap();

        // Add
        let id = schedule_add(&dir, "import.recipe.yaml", "@hourly").unwrap();
        assert!(id.starts_with("sched_import_recipe_"));

        // List
        let list = schedule_list(&dir).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].cron, "@hourly");
        assert_eq!(list[0].status, "active");

        // Remove
        schedule_remove(&dir, &id).unwrap();
        let list = schedule_list(&dir).unwrap();
        assert_eq!(list.len(), 0);

        // Remove non-existent
        assert!(schedule_remove(&dir, "nonexistent").is_err());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn add_invalid_cron_fails() {
        let dir = std::env::temp_dir().join("mc_tessera_schedule_cmd_test_bad");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(".tessera")).unwrap();

        let result = schedule_add(&dir, "recipe.yaml", "bad cron");
        assert!(result.is_err());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn generate_id_format() {
        let id = generate_schedule_id("path/to/my-recipe.yaml");
        assert!(id.starts_with("sched_my_recipe_"));
        // Should have secs and nanos parts
        let parts: Vec<&str> = id.split('_').collect();
        assert!(parts.len() >= 4);
    }
}
