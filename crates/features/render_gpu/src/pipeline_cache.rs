use std::fs::File;
use std::io::Read;
use std::path::Path;

use tracing::{debug, warn};

pub(crate) const MAX_PIPELINE_CACHE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PipelineCacheReadError {
    Io,
    TooLarge,
}

pub(crate) fn read_pipeline_cache_with_limit<R: Read>(
    reader: &mut R,
    capacity_hint: usize,
) -> Result<Vec<u8>, PipelineCacheReadError> {
    let mut limited_reader = reader.take(MAX_PIPELINE_CACHE_BYTES.saturating_add(1));
    let mut data = Vec::with_capacity(capacity_hint.min(MAX_PIPELINE_CACHE_BYTES as usize));
    if limited_reader.read_to_end(&mut data).is_err() {
        return Err(PipelineCacheReadError::Io);
    }
    if data.len() as u64 > MAX_PIPELINE_CACHE_BYTES {
        return Err(PipelineCacheReadError::TooLarge);
    }
    Ok(data)
}

pub(crate) fn load_pipeline_cache(path: &Path) -> Option<Vec<u8>> {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return None,
    };

    let file_size = metadata.len();
    if file_size == 0 {
        return None;
    }

    if file_size > MAX_PIPELINE_CACHE_BYTES {
        warn!(
            bytes = file_size,
            max_bytes = MAX_PIPELINE_CACHE_BYTES,
            path = %path.display(),
            "gpu init: skipped oversized pipeline cache file"
        );
        return None;
    }

    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return None,
    };

    match read_pipeline_cache_with_limit(&mut file, file_size as usize) {
        Ok(data) => Some(data),
        Err(PipelineCacheReadError::Io) => None,
        Err(PipelineCacheReadError::TooLarge) => {
            warn!(
                max_bytes = MAX_PIPELINE_CACHE_BYTES,
                path = %path.display(),
                "gpu init: skipped pipeline cache that exceeded read size limit"
            );
            None
        }
    }
}

pub(crate) fn save_pipeline_cache(
    cache_dir: &Path,
    adapter_info: &wgpu::AdapterInfo,
    cache: &wgpu::PipelineCache,
) {
    let Some(data) = cache.get_data() else {
        debug!("pipeline cache: no data to persist");
        return;
    };
    let Some(key) = wgpu::util::pipeline_cache_key(adapter_info) else {
        debug!("pipeline cache: adapter does not produce a cache key");
        return;
    };
    if let Err(e) = std::fs::create_dir_all(cache_dir) {
        warn!(error = %e, "pipeline cache: failed to create cache directory");
        return;
    }
    let path = cache_dir.join(key);
    let temp = path.with_extension("tmp");
    if let Err(e) = std::fs::write(&temp, &data) {
        warn!(error = %e, "pipeline cache: failed to write temp file");
        return;
    }
    if let Err(e) = std::fs::rename(&temp, &path) {
        warn!(error = %e, "pipeline cache: failed to rename temp to final");
        return;
    }
    debug!(
        bytes = data.len(),
        path = %path.display(),
        "pipeline cache: persisted to disk"
    );
}
