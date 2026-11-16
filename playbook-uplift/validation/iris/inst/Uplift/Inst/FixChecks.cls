/// Review fix checks for modules 03 (INST): reviewer finding 7, the stored developer mode. Uses the helpers and the
/// nc-inst-m fixtures of Uplift.Inst.Checks; prints OBS|nc-inst-fix-...| lines.
Class Uplift.Inst.FixChecks
{

/// A module loaded with -dev, then a plain load (no -dev, no -nodev) of the same version, a plain load of a newer
/// version that names an update package, and a plain update -path whose step fails, then succeeds.
/// Code: Utils/Module.cls#L1298-L1307 (stored DeveloperMode when no parameter), #L1219 (guard reads the parameter only),
/// Lifecycle/Module.cls#L122 (seeding tests the stored mode), #L1309-L1319 and #L1338 (developer-mode commits).
ClassMethod StoredDev()
{
    set C = "Uplift.Inst.Checks"
    set fix = $parameter(C, "FIX")
    do $classmethod(C, "Reset")
    do $classmethod(C, "Put", .kv, "devLoad100", $classmethod(C, "Run", "load -dev "_fix_"m/1.0.0/"))
    do $classmethod(C, "Put", .kv, "plainLoad100", $classmethod(C, "Run", "load "_fix_"m/1.0.0/"))
    set kv("version") = $classmethod(C, "Ver", "nc-inst-m")
    do $classmethod(C, "SimpleSteps", .kv, "NcInstM.Upd")
    do $classmethod(C, "Obs", "nc-inst-fix-stored-dev-plain-load", .kv)

    kill kv
    do $classmethod(C, "Put", .kv, "plainLoad200", $classmethod(C, "Run", "load "_fix_"m/2.0.0-good/"))
    set kv("version") = $classmethod(C, "Ver", "nc-inst-m")
    do $classmethod(C, "SimpleSteps", .kv, "NcInstM.Upd")
    do $classmethod(C, "Obs", "nc-inst-fix-stored-dev-load-newer", .kv)

    kill kv
    do $classmethod(C, "Put", .kv, "updateBad", $classmethod(C, "Run", "update nc-inst-m -path "_fix_"m/2.0.0-bad/"))
    do $classmethod(C, "MState", .kv)
    do $classmethod(C, "SimpleSteps", .kv, "NcInstM.Upd")
    do $classmethod(C, "Obs", "nc-inst-fix-stored-dev-update-path-fails", .kv)

    kill kv
    do $classmethod(C, "Put", .kv, "updateGood", $classmethod(C, "Run", "update nc-inst-m -path "_fix_"m/2.0.0-good/"))
    do $classmethod(C, "MState", .kv)
    do $classmethod(C, "SimpleSteps", .kv, "NcInstM.Upd")
    do $classmethod(C, "Obs", "nc-inst-fix-stored-dev-update-path-retry", .kv)
}

ClassMethod Main()
{
    write !, "===== FIX CHECK stored developer mode =====", !
    try {
        do ..StoredDev()
    } catch e {
        write "OBS|error-nc-inst-fix-stored-dev|error="_$classmethod("Uplift.Inst.Checks", "Clean", e.DisplayString())_";", !
    }
    while $tlevel { trollback }
}

}
