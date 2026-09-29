use ddo_model::DatasetVersion;
use std::path::{Path, PathBuf};
use std::process::Command;

fn data_files_dir_in(upstream_checkout_dir: &Path) -> PathBuf {
    upstream_checkout_dir.join("Output").join("DataFiles")
}

pub fn default_data_files_dir(checkout_root: &Path) -> PathBuf {
    data_files_dir_in(&checkout_root.join("upstream"))
}

pub fn dataset_version(data_files_dir: &Path, upstream_sha: Option<String>) -> DatasetVersion {
    let upstream_sha =
        upstream_sha.unwrap_or_else(|| head_commit_sha(data_files_dir).unwrap_or_else(|| "unknown".to_string()));
    DatasetVersion { upstream_sha, built_at: current_utc_timestamp() }
}

fn head_commit_sha(repository_dir: &Path) -> Option<String> {
    let output = Command::new("git").arg("-C").arg(repository_dir).args(["rev-parse", "HEAD"]).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn current_utc_timestamp() -> String {
    let seconds_since_epoch =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let days_since_epoch = (seconds_since_epoch / 86_400) as i64;
    let (hour, minute, second) =
        ((seconds_since_epoch % 86_400) / 3600, (seconds_since_epoch % 3600) / 60, seconds_since_epoch % 60);
    let days_since_era_zero = days_since_epoch + 719_468;
    let era = days_since_era_zero.div_euclid(146_097);
    let day_of_era = days_since_era_zero.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
    let month = if month_from_march < 10 { month_from_march + 3 } else { month_from_march - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}
