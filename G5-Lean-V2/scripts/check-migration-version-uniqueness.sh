#!/usr/bin/env bash
set -euo pipefail

check_migration_version_uniqueness() {
  local migration_dir="${1:-backend/g5-api/src/db/migrations}"
  if [[ ! -d "${migration_dir}" ]]; then
    echo "Migration directory does not exist: ${migration_dir}" >&2
    return 1
  fi

  shopt -s nullglob
  local migration_files=("${migration_dir}"/*.sql)
  if (( ${#migration_files[@]} == 0 )); then
    echo "Migration directory contains no .sql files: ${migration_dir}" >&2
    return 1
  fi

  local migration_file file_name prefix version
  local highest_version=0
  declare -A first_file_by_version=()
  declare -a problems=()

  for migration_file in "${migration_files[@]}"; do
    file_name="${migration_file##*/}"
    if [[ "${file_name}" != *_* ]]; then
      problems+=("${file_name}: migration filename must contain a numeric prefix followed by an underscore")
      continue
    fi

    prefix="${file_name%%_*}"
    if [[ ! "${prefix}" =~ ^[0-9]+$ ]]; then
      problems+=("${file_name}: migration filename prefix must be numeric")
      continue
    fi

    version=$((10#${prefix}))
    if [[ -n "${first_file_by_version[${version}]+present}" ]]; then
      problems+=("duplicate SQLx migration version ${version}: ${first_file_by_version[${version}]} and ${file_name}")
      continue
    fi

    first_file_by_version[${version}]="${file_name}"
    if (( version > highest_version )); then
      highest_version=${version}
    fi
  done

  if (( ${#problems[@]} > 0 )); then
    echo "Migration catalog validation failed:" >&2
    printf '  - %s\n' "${problems[@]}" >&2
    return 1
  fi

  printf 'SQLx migration versions are unique: %d files, highest version %d\n' \
    "${#migration_files[@]}" "${highest_version}"
}

check_migration_version_uniqueness "$@"
