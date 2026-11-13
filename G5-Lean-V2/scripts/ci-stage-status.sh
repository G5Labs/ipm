#!/bin/sh
# Test steps may continue through cleanup, but their Cloud Build status must
# reflect the recorded test result. The final gate requires every receipt.
set -eu

mode=$1
shift
case "$mode" in
  report)
    original_status=$1
    receipt=$2
    key=$3
    if [ "$original_status" -ne 0 ] || [ ! -f "$receipt" ]; then
      printf '%s=failed\n' "$key" > "$receipt"
    fi
    result=$(cat "$receipt")
    if [ "$result" = "$key=passed" ]; then
      printf 'CI stage %s: PASSED\n' "$key"
    else
      printf 'CI stage %s: FAILED (receipt: %s)\n' "$key" "$result" >&2
      # Normalize malformed receipts so downstream cleanup captures logs.
      printf '%s=failed\n' "$key" > "$receipt"
      exit 1
    fi
    ;;
  assert)
    directory=$1
    manifest=$2
    failed=0
    count=0
    while read -r step key filename; do
      [ -n "$step" ] || continue
      count=$((count + 1))
      receipt="$directory/$filename"
      if [ ! -f "$receipt" ]; then
        printf 'CI stage %s (%s): MISSING RESULT\n' "$key" "$step" >&2
        failed=1
      elif [ "$(cat "$receipt")" != "$key=passed" ]; then
        printf 'CI stage %s (%s): FAILED OR INVALID RESULT\n' "$key" "$step" >&2
        failed=1
      else
        printf 'CI stage %s (%s): PASSED\n' "$key" "$step"
      fi
    done < "$manifest"
    [ "$count" -gt 0 ] || { echo 'CI stage manifest is empty' >&2; exit 1; }
    [ "$failed" -eq 0 ] || { echo 'Required CI stages did not pass' >&2; exit 1; }
    printf 'All %s required CI stages passed\n' "$count"
    ;;
  *) echo "Unknown CI status command: $mode" >&2; exit 2 ;;
esac
