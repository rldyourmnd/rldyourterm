#!/usr/bin/env bash
set -euo pipefail

OUT_DIR="${1:-codeql-rust-diagnostics}"
DB_ROOT="${CODEQL_DATABASE_ROOT:-/home/runner/work/_temp/codeql_databases/rust}"
RESULTS_ROOT="${DB_ROOT}/results/codeql/rust-queries/queries"
EXTRACTOR_DIAG_DIR="${DB_ROOT}/diagnostic/extractors/rust"

mkdir -p "${OUT_DIR}"

copy_if_exists() {
  local src="$1"
  local rel="$2"
  local dst="${OUT_DIR}/${rel}"
  if [ -f "${src}" ]; then
    mkdir -p "$(dirname "${dst}")"
    cp "${src}" "${dst}"
    return 0
  fi
  return 1
}

detect_codeql_bin() {
  if command -v codeql >/dev/null 2>&1; then
    command -v codeql
    return 0
  fi

  local candidate
  candidate="$(ls -1d /opt/hostedtoolcache/CodeQL/*/x64/codeql/codeql 2>/dev/null | sort -V | tail -n 1 || true)"
  if [ -n "${candidate}" ] && [ -x "${candidate}" ]; then
    echo "${candidate}"
    return 0
  fi

  return 1
}

decode_bqrs() {
  local codeql_bin="$1"
  local src_rel="$2"
  local dst_rel="$3"
  local src="${RESULTS_ROOT}/${src_rel}"
  local dst="${OUT_DIR}/${dst_rel}"

  if [ ! -f "${src}" ]; then
    return 0
  fi

  mkdir -p "$(dirname "${dst}")"
  if ! "${codeql_bin}" bqrs decode --format=csv --output="${dst}" "${src}" >> "${OUT_DIR}/decode.log" 2>&1; then
    rm -f "${dst}"
    local json_dst="${dst%.csv}.json"
    "${codeql_bin}" bqrs decode --format=json --output="${json_dst}" "${src}" >> "${OUT_DIR}/decode.log" 2>&1 || true
  fi
}

extract_metric_value() {
  local csv_path="$1"
  if [ ! -f "${csv_path}" ]; then
    echo "n/a"
    return 0
  fi

  local value
  value="$(awk -F, 'NR==2 { gsub(/"/, "", $NF); print $NF }' "${csv_path}")"
  if [ -z "${value}" ]; then
    echo "n/a"
  else
    echo "${value}"
  fi
}

# Copy raw BQRS diagnostics for offline inspection.
copy_if_exists "${RESULTS_ROOT}/summary/NumberOfFilesExtractedWithErrors.bqrs" "raw/summary/NumberOfFilesExtractedWithErrors.bqrs" || true
copy_if_exists "${RESULTS_ROOT}/summary/NumberOfSuccessfullyExtractedFiles.bqrs" "raw/summary/NumberOfSuccessfullyExtractedFiles.bqrs" || true
copy_if_exists "${RESULTS_ROOT}/diagnostics/ExtractionErrors.bqrs" "raw/diagnostics/ExtractionErrors.bqrs" || true
copy_if_exists "${RESULTS_ROOT}/diagnostics/ExtractionWarnings.bqrs" "raw/diagnostics/ExtractionWarnings.bqrs" || true
copy_if_exists "${RESULTS_ROOT}/diagnostics/UnextractedElements.bqrs" "raw/diagnostics/UnextractedElements.bqrs" || true
copy_if_exists "${RESULTS_ROOT}/diagnostics/UnresolvedMacroCalls.bqrs" "raw/diagnostics/UnresolvedMacroCalls.bqrs" || true
copy_if_exists "${RESULTS_ROOT}/telemetry/ExtractorInformation.bqrs" "raw/telemetry/ExtractorInformation.bqrs" || true

if [ -d "${EXTRACTOR_DIAG_DIR}" ]; then
  tar -C "${EXTRACTOR_DIAG_DIR}" -czf "${OUT_DIR}/extractor-diagnostics.tar.gz" .
fi

CODEQL_BIN=""
if CODEQL_BIN="$(detect_codeql_bin)"; then
  decode_bqrs "${CODEQL_BIN}" "summary/NumberOfFilesExtractedWithErrors.bqrs" "decoded/summary/NumberOfFilesExtractedWithErrors.csv"
  decode_bqrs "${CODEQL_BIN}" "summary/NumberOfSuccessfullyExtractedFiles.bqrs" "decoded/summary/NumberOfSuccessfullyExtractedFiles.csv"
  decode_bqrs "${CODEQL_BIN}" "diagnostics/ExtractionErrors.bqrs" "decoded/diagnostics/ExtractionErrors.csv"
  decode_bqrs "${CODEQL_BIN}" "diagnostics/ExtractionWarnings.bqrs" "decoded/diagnostics/ExtractionWarnings.csv"
  decode_bqrs "${CODEQL_BIN}" "diagnostics/UnextractedElements.bqrs" "decoded/diagnostics/UnextractedElements.csv"
  decode_bqrs "${CODEQL_BIN}" "diagnostics/UnresolvedMacroCalls.bqrs" "decoded/diagnostics/UnresolvedMacroCalls.csv"
fi

errors_count="$(extract_metric_value "${OUT_DIR}/decoded/summary/NumberOfFilesExtractedWithErrors.csv")"
success_count="$(extract_metric_value "${OUT_DIR}/decoded/summary/NumberOfSuccessfullyExtractedFiles.csv")"

{
  echo "# CodeQL Rust Extraction Diagnostics"
  echo
  echo "- Database root: \`${DB_ROOT}\`"
  if [ -n "${CODEQL_BIN}" ]; then
    echo "- CodeQL CLI: \`${CODEQL_BIN}\`"
  else
    echo "- CodeQL CLI: not found in PATH/toolcache (raw BQRS only)"
  fi
  echo "- Extracted with errors: \`${errors_count}\`"
  echo "- Extracted without error: \`${success_count}\`"
  echo
  echo "## Raw BQRS payloads"
  (find "${OUT_DIR}/raw" -type f 2>/dev/null || true) | sort | sed "s#^${OUT_DIR}/#- #"
  echo
  echo "## Decoded diagnostics"
  (find "${OUT_DIR}/decoded" -type f 2>/dev/null || true) | sort | sed "s#^${OUT_DIR}/#- #"
  if [ -f "${OUT_DIR}/decode.log" ]; then
    echo
    echo "## Decode log"
    echo "- \`decode.log\` is included in the artifact."
  fi
} > "${OUT_DIR}/summary.md"
