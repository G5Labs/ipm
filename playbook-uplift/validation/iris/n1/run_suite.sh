#!/bin/bash
# Runs every per-class suite script in validation/iris/n1/suite against $CONTAINER, one IRIS session per class.
# Logs: validation/logs/95-suite-<script>.log
EXP=/Users/ziniuw/Desktop/projects/G5-benchmark/results/experiments/ipm-playbook-uplift
export CONTAINER=${CONTAINER:-ipm-uplift-n1-test}
for s in "$@"; do
  n=$(basename "$s" .script)
  start=$(date +%s)
  timeout 900 "$EXP/scripts/iris_run.sh" "$s" "$EXP/validation/logs/95-suite-$n.log"; rc=$?
  echo "$n rc=$rc secs=$(( $(date +%s) - start )) $(grep -aoE '^RESULT(-SUITE)? .{0,160}' "$EXP/validation/logs/95-suite-$n.log" | head -1)"
done
