/// Observation helpers for the follow-up native cases. Each value is written as
/// "OBS <case> <key>=<value>" between "CASE-BEGIN <case>" and "CASE-END <case>".
Class FUNative.Obs
{

ClassMethod Begin(case As %String)
{
    write !,"CASE-BEGIN ",case," ",$zdatetime($horolog,3)," job=",$job,!
}

ClassMethod End(case As %String)
{
    write !,"CASE-END ",case," ",$zdatetime($horolog,3),!
}

ClassMethod Obs(
	case As %String,
	key As %String,
	value As %String)
{
    write !,"OBS ",case," ",key,"=",value,!
}

/// Runs one IPM shell command and records its status.
ClassMethod Run(
	case As %String,
	label As %String,
	cmd As %String) As %Status
{
    write !,"CMD ",case," ",cmd,!
    set sc = $$$OK
    try {
        set sc = ##class(%IPM.Main).Shell(cmd)
    } catch ex {
        set sc = ex.AsStatus()
    }
    do ..Obs(case, label_".ok", $system.Status.IsOK(sc))
    if $$$ISERR(sc) {
        do ..Obs(case, label_".err", $extract($translate($system.Status.GetErrorText(sc), $char(13,10), "  "), 1, 300))
    }
    do ..Obs(case, label_".tlevel", $tlevel)
    quit sc
}

ClassMethod MaxHistId() As %Integer
{
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT MAX(ID) AS m FROM %IPM_General.History")
    do rs.%Next()
    quit +rs.m
}

/// Module record, compiled class <Pkg>.Main and stored document version for one module name.
ClassMethod Mod(
	case As %String,
	prefix As %String,
	name As %String)
{
    set p = prefix_name_"."
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT VersionString, DeveloperMode, LastUpdated FROM %IPM_Storage.ModuleItem WHERE Name = ?", name)
    if rs.%Next() {
        do ..Obs(case, p_"present", 1)
        do ..Obs(case, p_"version", rs.%Get("VersionString"))
        do ..Obs(case, p_"dev", +rs.%Get("DeveloperMode"))
        do ..Obs(case, p_"last_updated", rs.%Get("LastUpdated"))
    } else {
        do ..Obs(case, p_"present", 0)
    }
    do ..Obs(case, p_"class", ##class(%Dictionary.CompiledClass).%ExistsId(..PkgOf(name)_".Main"))
}

ClassMethod Mods(
	case As %String,
	prefix As %String,
	names As %String)
{
    for i=1:1:$length(names, ",") {
        set n = $piece(names, ",", i)
        continue:n=""
        do ..Mod(case, prefix, n)
    }
}

/// Every History row after the baseline ID, in ID order: package, success, committed, finalized, ended.
ClassMethod Rows(
	case As %String,
	base As %Integer)
{
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT ID, Package, Success, Committed, Finalized, TimeEnd FROM %IPM_General.History WHERE ID > ? ORDER BY ID", base)
    set n = 0
    while rs.%Next() {
        set n = n + 1
        set p = "row"_n_"."
        do ..Obs(case, p_"package", rs.%Get("Package"))
        do ..Obs(case, p_"success", $select(rs.%Get("Success")=1:"OK", rs.%Get("Success")=0:"ZERO", 1:"ERR"))
        do ..Obs(case, p_"committed", +rs.%Get("Committed"))
        do ..Obs(case, p_"finalized", +rs.%Get("Finalized"))
        do ..Obs(case, p_"ended", rs.%Get("TimeEnd") '= "")
    }
    do ..Obs(case, "rows", n)
    do ..Obs(case, "ids_allocated", +$get(^IPM.General.HistoryD) - base)
}

ClassMethod PkgOf(name) As %String
{
    set out = ""
    for i=1:1:$length(name, "-") {
        set p = $piece(name, "-", i)
        set out = out_$zconvert($extract(p), "U")_$extract(p, 2, *)
    }
    quit out
}

/// Rows returned by %IPM.General.History:GetHistory for a filter given as "col=value;col=value".
ClassMethod Find(
	case As %String,
	label As %String,
	spec As %String,
	ascend As %Boolean = 0,
	limit As %Integer = 0)
{
    kill filter
    for i=1:1:$length(spec, ";") {
        set kv = $piece(spec, ";", i)
        continue:kv=""
        set filter($piece(kv, "=", 1)) = $piece(kv, "=", 2, *)
    }
    set err = ""
    set n = 0, pk = ""
    try {
        set rs = ##class(%IPM.General.History).GetHistory(.filter, ascend, limit)
        if rs.%SQLCODE < 0 {
            set err = "SQLCODE "_rs.%SQLCODE
        }
        while rs.%Next() {
            set n = n + 1
            set pk = pk_$listbuild(rs.%Get("Package")_"#"_rs.%Get("ID"))
        }
    } catch ex {
        set err = ex.DisplayString()
    }
    do ..Obs(case, label_".n", n)
    do ..Obs(case, label_".rows", $listtostring(pk, ","))
    do ..Obs(case, label_".err", err)
}

/// Inserts one finalized synthetic History row (test fixture) and returns its ID.
ClassMethod Seed(
	package As %String,
	version As %String,
	daysAgo As %Integer) As %Integer
{
    set h = ##class(%IPM.General.History).%New()
    set h.Action = "load", h.Package = package, h.CommandString = "followup seed"
    set h.Version = ##class(%IPM.General.SemanticVersion).FromString(version)
    set h.TimeStart = $zdatetime(($piece($horolog, ",") - daysAgo)_",43200", 3)
    set h.Success = 1, h.Committed = 1, h.Finalized = 1
    $$$ThrowOnError(h.%Save())
    quit h.%Id()
}

/// Count of History rows whose package starts with a prefix.
ClassMethod CountPrefix(prefix As %String) As %Integer
{
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT COUNT(*) AS n FROM %IPM_General.History WHERE Package %STARTSWITH ?", prefix)
    do rs.%Next()
    quit +rs.n
}

ClassMethod CountAll() As %Integer
{
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT COUNT(*) AS n FROM %IPM_General.History")
    do rs.%Next()
    quit +rs.n
}

}
