use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tracing::{debug, error, info, warn};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{fmt::Layer, prelude::*, registry::Registry, EnvFilter};

static MCP_LOG_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Initialize logging for MCP server with file rotation
pub fn init_mcp_logging(base_dir: PathBuf, debug_mode: bool) -> Result<(), anyhow::Error> {
	// Create a unique log directory based on project path
	let project_hash = {
		let mut hasher = Sha256::new();
		hasher.update(base_dir.to_string_lossy().as_bytes());
		format!("{:x}", hasher.finalize())[..12].to_string()
	};

	// Get project name for better identification
	let project_name = base_dir
		.file_name()
		.and_then(|n| n.to_str())
		.unwrap_or("unknown");

	// Ensure .octocode/logs/project_name_hash directory exists
	let log_dir_name = format!("{}_{}", project_name, project_hash);
	let log_dir = base_dir.join(".octocode").join("logs").join(log_dir_name);
	std::fs::create_dir_all(&log_dir)?;

	// Store log directory for potential future use
	MCP_LOG_DIR
		.set(log_dir.clone())
		.map_err(|_| anyhow::anyhow!("Failed to set log directory"))?;

	// Cross-platform way to create a "latest" indicator
	let latest_file = base_dir.join(".octocode").join("logs").join("latest.txt");
	std::fs::write(&latest_file, log_dir.to_string_lossy().as_bytes()).unwrap_or_else(|e| {
		eprintln!("Warning: Could not create latest log indicator: {}", e);
	});

	// Create rotating file appender
	let file_appender = RollingFileAppender::new(Rotation::DAILY, &log_dir, "mcp_server.log");

	// Set up environment filter with sensible defaults
	let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
		if debug_mode {
			EnvFilter::new("debug")
		} else {
			EnvFilter::new("info")
		}
	});

	// File layer with JSON formatting for structured logs
	let file_layer = Layer::new()
		.with_writer(file_appender)
		.with_ansi(false)
		.with_target(true)
		.with_file(true)
		.with_line_number(true)
		.with_thread_ids(true)
		.with_thread_names(true)
		.json();

	// Console layer (only in debug mode)
	let console_layer = if debug_mode {
		Some(
			Layer::new()
				.with_writer(std::io::stderr)
				.with_ansi(true)
				.with_target(false)
				.with_thread_ids(false)
				.with_thread_names(false),
		)
	} else {
		None
	};

	// Create registry with layers
	let registry = Registry::default().with(file_layer).with(env_filter);

	if let Some(console) = console_layer {
		registry.with(console).init();
	} else {
		registry.init();
	}

	info!(
		project_path = %base_dir.display(),
		project_name = project_name,
		log_directory = %log_dir.display(),
		debug_mode = debug_mode,
		"MCP Server logging initialized"
	);

	Ok(())
}

/// Log MCP server request details
pub fn log_mcp_request(
	method: &str,
	params: Option<&serde_json::Value>,
	request_id: Option<&serde_json::Value>,
) {
	info!(
		method = method,
		request_id = ?request_id,
		params_size = params.map(|p| p.to_string().len()).unwrap_or(0),
		"MCP Request received"
	);

	if let Some(params) = params {
		debug!(
			method = method,
			params = %params,
			"MCP Request parameters"
		);
	}
}

/// Log MCP server response
pub fn log_mcp_response(
	method: &str,
	success: bool,
	request_id: Option<&serde_json::Value>,
	duration_ms: Option<u64>,
) {
	if success {
		info!(
			method = method,
			request_id = ?request_id,
			duration_ms = duration_ms,
			"MCP Request processed successfully"
		);
	} else {
		warn!(
			method = method,
			request_id = ?request_id,
			duration_ms = duration_ms,
			"MCP Request processing failed"
		);
	}
}

/// Log critical errors with context
pub fn log_critical_error(context: &str, error: &dyn std::error::Error) {
	error!(
		context = context,
		error = %error,
		error_chain = ?error.source(),
		"Critical error in MCP server"
	);
}

/// Log file watcher events
pub fn log_watcher_event(event_type: &str, path: Option<&std::path::Path>, count: usize) {
	debug!(
		event_type = event_type,
		path = ?path,
		event_count = count,
		"File watcher event"
	);
}

/// Log indexing operations
pub fn log_indexing_operation(
	operation: &str,
	file_count: Option<usize>,
	duration_ms: Option<u64>,
	success: bool,
) {
	if success {
		info!(
			operation = operation,
			file_count = file_count,
			duration_ms = duration_ms,
			"Indexing operation completed"
		);
	} else {
		error!(
			operation = operation,
			file_count = file_count,
			duration_ms = duration_ms,
			"Indexing operation failed"
		);
	}
}

/// Get the current log directory
/// Get the current log directory
pub fn get_log_directory() -> Option<PathBuf> {
	MCP_LOG_DIR.get().cloned()
}

/// Get all log directories for MCP server
pub fn get_all_log_directories(base_dir: &std::path::Path) -> Result<Vec<PathBuf>, std::io::Error> {
	let logs_dir = base_dir.join(".octocode").join("logs");

	if !logs_dir.exists() {
		return Ok(Vec::new());
	}

	// Read all subdirectories in the logs directory
	let mut directories = Vec::new();
	for entry in std::fs::read_dir(logs_dir)? {
		let entry = entry?;
		let path = entry.path();

		if path.is_dir() {
			if let Some(name) = path.file_name() {
				let name_str = name.to_string_lossy();
				// Skip hidden directories and files
				if !name_str.starts_with('.') && !name_str.ends_with(".txt") {
					directories.push(path);
				}
			}
		}
	}

	// Sort by modification time (newest first)
	directories.sort_by(|a, b| {
		let a_time = a
			.metadata()
			.and_then(|m| m.modified())
			.unwrap_or(std::time::SystemTime::UNIX_EPOCH);
		let b_time = b
			.metadata()
			.and_then(|m| m.modified())
			.unwrap_or(std::time::SystemTime::UNIX_EPOCH);
		b_time.cmp(&a_time)
	});

	Ok(directories)
}

/// Print log directory information
pub fn print_log_directories(base_dir: &Path) -> Result<(), std::io::Error> {
	println!("MCP Server Log Directories:");
	for (index, log_dir) in get_all_log_directories(base_dir)?.iter().enumerate() {
		println!("{}: {}", index + 1, log_dir.display());
	}
	Ok(())
}
