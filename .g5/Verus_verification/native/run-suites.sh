#!/usr/bin/env bash
# Run product %UnitTest suites one at a time through run-iris.sh and evaluate each.
# Usage: run-suites.sh verify|test CaseName [CaseName ...]
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
kind="$1"; shift
for tc in "$@"; do
    short="${tc##*.}"
    script="$here/cos/suite-$kind-$short.cos"
    {
        if [ "$kind" = verify ]; then
            echo 'do $system.OBJ.Load("/home/irisowner/zpm/tests/integration_tests/Test/PM/Integration/Base.cls","ck-d")'
        fi
        echo "zpm \"zpm $kind -only -DUnitTest.Case=$tc\""
        if [ "$kind" = verify ]; then
            echo 'set utns = "USER-VERIFY"'
            cat "$here/print-last-unittest.cosfrag"
        else
            echo 'set utns = "USER"'
            cat "$here/print-last-unittest.cosfrag"
            echo 'set $namespace = "USER"'
            echo 'set utns = "USER-VERIFY"'
            cat "$here/print-last-unittest.cosfrag"
        fi
        if [ "$short" = CLI ]; then
            echo 'set $namespace = "USER"'
            echo 'write !,"OBS S-CLI-post HistoryRetain=",##class(%IPM.Repo.UniversalSettings).GetHistoryRetain(),!'
        fi
        echo 'write !,"NATIVE-END",!'
        echo 'halt'
    } > "$script"
    line=$("$here/run-iris.sh" exec "suite-$kind-$short" "$script")
    echo "$line"
    run_id="${line%% *}"
    python3 -I "$here/evaluate_suite.py" "$here/../results/native/$run_id" "$tc" --index "product suite $tc"
    "$here/run-iris.sh" purge-journals
done
