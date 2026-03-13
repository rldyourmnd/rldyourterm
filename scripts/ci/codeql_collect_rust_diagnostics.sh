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
  candidate="$(find /opt/hostedtoolcache/CodeQL -path '*/x64/codeql/codeql' -type f 2>/dev/null | sort -V | tail -n 1 || true)"
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

count_csv_rows() {
  local csv_path="$1"
  if [ ! -f "${csv_path}" ]; then
    echo "0"
    return 0
  fi

  awk 'NR > 1 && length($0) > 0 { c++ } END { print c + 0 }' "${csv_path}"
}

classify_extraction_warnings() {
  local src_csv="$1"
  local actionable_csv="$2"
  local benign_csv="$3"

  if [ ! -f "${src_csv}" ]; then
    return 0
  fi

  local header
  header="$(head -n 1 "${src_csv}" || true)"
  printf '%s\n' "${header}" > "${actionable_csv}"
  printf '%s\n' "${header}" > "${benign_csv}"

  awk -v actionable="${actionable_csv}" -v benign="${benign_csv}" '
    NR == 1 { next }
    NF == 0 { next }
    {
      is_semantic = ($0 ~ /semantic analyzer unavailable \(not included in files loaded from manifest\)/)
      is_generated = ($0 ~ /\/target\/[^"]*\/build\/[^"]*\/out\/[^"]*/)

      if (is_semantic && is_generated) {
        print >> benign
      } else {
        print >> actionable
      }
    }
  ' "${src_csv}"
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
errors_rows="$(count_csv_rows "${OUT_DIR}/decoded/diagnostics/ExtractionErrors.csv")"
warnings_rows="$(count_csv_rows "${OUT_DIR}/decoded/diagnostics/ExtractionWarnings.csv")"

actionable_warnings_csv="${OUT_DIR}/decoded/diagnostics/ActionableExtractionWarnings.csv"
benign_warnings_csv="${OUT_DIR}/decoded/diagnostics/BenignGeneratedExtractionWarnings.csv"
classify_extraction_warnings "${OUT_DIR}/decoded/diagnostics/ExtractionWarnings.csv" "${actionable_warnings_csv}" "${benign_warnings_csv}"
actionable_warnings_rows="$(count_csv_rows "${actionable_warnings_csv}")"
benign_warnings_rows="$(count_csv_rows "${benign_warnings_csv}")"

cat > "${OUT_DIR}/status.env" <<EOF
CODEQL_EXTRACTED_WITH_ERRORS_METRIC=${errors_count}
CODEQL_EXTRACTED_WITHOUT_ERRORS_METRIC=${success_count}
CODEQL_EXTRACTION_ERROR_ROWS=${errors_rows}
CODEQL_EXTRACTION_WARNING_ROWS=${warnings_rows}
CODEQL_ACTIONABLE_EXTRACTION_WARNING_ROWS=${actionable_warnings_rows}
CODEQL_BENIGN_EXTRACTION_WARNING_ROWS=${benign_warnings_rows}
EOF

{
  echo "# CodeQL Rust Extraction Diagnostics"
  echo
  echo "- Database root: \`${DB_ROOT}\`"
  if [ -n "${CODEQL_BIN}" ]; then
    echo "- CodeQL CLI: \`${CODEQL_BIN}\`"
  else
    echo "- CodeQL CLI: not found in PATH/toolcache (raw BQRS only)"
  fi
  echo "- Extracted with errors (CodeQL metric): \`${errors_count}\`"
  echo "- Extracted without error (CodeQL metric): \`${success_count}\`"
  echo "- ExtractionErrors rows: \`${errors_rows}\`"
  echo "- ExtractionWarnings rows: \`${warnings_rows}\`"
  echo "- Actionable extraction warnings: \`${actionable_warnings_rows}\`"
  echo "- Benign generated extraction warnings: \`${benign_warnings_rows}\`"
  echo
  echo "## Raw BQRS payloads"
  (find "${OUT_DIR}/raw" -type f 2>/dev/null || true) | sort | sed "s#^${OUT_DIR}/#- #"
  echo
  echo "## Decoded diagnostics"
  (find "${OUT_DIR}/decoded" -type f 2>/dev/null || true) | sort | sed "s#^${OUT_DIR}/#- #"
  echo
  echo "## Machine-readable status"
  echo "- \`status.env\` is included in the artifact."
  if [ -f "${OUT_DIR}/decode.log" ]; then
    echo
    echo "## Decode log"
    echo "- \`decode.log\` is included in the artifact."
  fi
} > "${OUT_DIR}/summary.md"

if [ "${CODEQL_RUST_DIAGNOSTICS_ENFORCE:-0}" = "1" ] || [ "${CODEQL_RUST_DIAGNOSTICS_FAIL_ON_ACTIONABLE:-0}" = "1" ]; then
  if [ "${errors_rows}" -gt 0 ] || [ "${actionable_warnings_rows}" -gt 0 ]; then
    printf '::error::CodeQL actionable extraction diagnostics detected (errors=%s actionable_warnings=%s)\n' \
      "${errors_rows}" "${actionable_warnings_rows}" >&2
    exit 1
  fi
fi
