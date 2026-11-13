/// Observation helpers for B2 native checks. Not product code.
/// Every observation is printed as one line: OBS <case> <field>=<value>
/// The evaluator (native/evaluate.py) reads only these lines, CASE-BEGIN/CASE-END markers and NATIVE-END.
Class B2Native.Util
{

ClassMethod Obs(case As %String, field As %String, value As %String = "")
{
    set value = $translate($get(value), $char(13,10,9), "   ")
    write !,"OBS ",case," ",field,"=",value,!
}

ClassMethod Begin(case As %String)
{
    write !,"CASE-BEGIN ",case," ",$zdatetime($horolog,3)," job=",$job,!
}

ClassMethod End(case As %String)
{
    write !,"CASE-END ",case," ",$zdatetime($horolog,3),!
}

ClassMethod StatusText(sc) As %String
{
    if $get(sc) = "" { quit "<empty>" }
    if sc = 0 { quit "ZERO" }
    if sc = 1 { quit "OK" }
    set text = ""
    try {
        set text = $system.Status.GetErrorText(sc)
    } catch {
        set text = "<undecodable>"
    }
    quit "ERR:"_$extract(text, 1, 300)
}

/// OK, ERR or ZERO (the in-progress placeholder 0)
ClassMethod StatusKind(sc) As %String
{
    if $get(sc) = "" { quit "EMPTY" }
    if sc = 0 { quit "ZERO" }
    if $$$ISOK(sc) { quit "OK" }
    quit "ERR"
}

/// Run an IPM shell command at the real entry point and record its returned %Status.
ClassMethod Run(
	case As %String,
	label As %String,
	cmd As %String) As %Status
{
    write !,"CMD ",case," ",label," ",cmd,!
    set sc = $$$OK
    try {
        set sc = ##class(%IPM.Main).Shell(cmd)
    } catch ex {
        set sc = ex.AsStatus()
        do ..Obs(case, label_".thrown", 1)
    }
    do ..Obs(case, label_".ok", $$$ISOK(sc))
    if $$$ISERR(sc) {
        do ..Obs(case, label_".err", $extract($system.Status.GetErrorText(sc), 1, 400))
    }
    do ..Obs(case, label_".tlevel", $tlevel)
    quit sc
}

/// Same as Run, but the command's terminal output is captured to a file, stripped of ANSI codes,
/// echoed as OUT lines and returned in lines.
ClassMethod RunCapture(
	case As %String,
	label As %String,
	cmd As %String,
	Output lines) As %Status
{
    kill lines
    set lines = 0
    set file = "/tmp/b2cap-"_$job_".txt"
    do ##class(%File).Delete(file)
    write !,"CMD ",case," ",label," ",cmd,!
    set dev = $io
    open file:("WNS"):5
    if '$test {
        do ..Obs(case, label_".capture", "open-failed")
        quit ..Run(case, label, cmd)
    }
    set sc = $$$OK
    use file
    try {
        set sc = ##class(%IPM.Main).Shell(cmd)
    } catch ex {
        set sc = ex.AsStatus()
    }
    use dev
    close file
    set stream = ##class(%Stream.FileCharacter).%New()
    set stream.Filename = file
    set matcher = ##class(%Regex.Matcher).%New("\x1B\[[0-9;]*m")
    while 'stream.AtEnd {
        set line = stream.ReadLine()
        set matcher.Text = line
        set line = matcher.ReplaceAll("")
        set lines($increment(lines)) = line
        write "OUT ",case," ",label,"| ",line,!
    }
    kill stream
    do ..Obs(case, label_".ok", $$$ISOK(sc))
    if $$$ISERR(sc) {
        do ..Obs(case, label_".err", $extract($system.Status.GetErrorText(sc), 1, 400))
    }
    quit sc
}

/// Number of record lines in captured `history find` output (lines starting with "<ID> <action> ").
ClassMethod CountFindRows(ByRef lines) As %Integer
{
    set n = 0
    for i=1:1:$get(lines) {
        if $match($get(lines(i)), "^\s*\d+ (load|install|uninstall|update) .*") {
            set n = n + 1
        }
    }
    quit n
}

ClassMethod MaxHistId() As %Integer
{
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT MAX(ID) AS m FROM %IPM_General.History")
    do rs.%Next()
    quit +rs.%Get("m")
}

ClassMethod SqlCount(
	sql As %String,
	args...) As %Integer
{
    set rs = ##class(%SQL.Statement).%ExecDirect(, sql, args...)
    if rs.%SQLCODE < 0 {
        quit "SQLERR"_rs.%SQLCODE
    }
    do rs.%Next()
    quit +rs.%GetData(1)
}

ClassMethod VersionFrom(rs) As %String
{
    set v = rs.%Get("Version_Major")_"."_rs.%Get("Version_Minor")_"."_rs.%Get("Version_Patch")
    if rs.%Get("Version_Prerelease") '= "" {
        set v = v_"-"_rs.%Get("Version_Prerelease")
    }
    if rs.%Get("Version_Build") '= "" {
        set v = v_"+"_rs.%Get("Version_Build")
    }
    quit v
}

/// Print one persisted %IPM_General.History row as OBS lines under prefix.
ClassMethod ObsRow(
	case As %String,
	prefix As %String,
	rs)
{
    do ..Obs(case, prefix_"ID", rs.%Get("ID"))
    do ..Obs(case, prefix_"Action", rs.%Get("Action"))
    do ..Obs(case, prefix_"Package", rs.%Get("Package"))
    do ..Obs(case, prefix_"Version", ..VersionFrom(rs))
    do ..Obs(case, prefix_"SuccessKind", ..StatusKind(rs.%Get("Success")))
    do ..Obs(case, prefix_"SuccessText", ..StatusText(rs.%Get("Success")))
    do ..Obs(case, prefix_"Committed", rs.%Get("Committed"))
    do ..Obs(case, prefix_"Finalized", rs.%Get("Finalized"))
    do ..Obs(case, prefix_"TimeEndSet", rs.%Get("TimeEnd") '= "")
    do ..Obs(case, prefix_"SourceName", rs.%Get("SourceName"))
    do ..Obs(case, prefix_"CommandString", rs.%Get("CommandString"))
    set phases = rs.%Get("Phases")
    set out = ""
    for i=1:1:$listlength(phases) {
        set ph = $listget($listget(phases, i), 1)
        set out = out_$select(out="":"",1:",")_$listget(ph,1)_":"_$select($listget(ph,3)="":"open",1:"ended")_":"_..StatusKind($listget(ph,4))
    }
    do ..Obs(case, prefix_"Phases", out)
}

/// Persisted history rows with ID > sinceId, in ID order, as rowN.* fields. Also prints rows=<count>.
ClassMethod HistSince(
	case As %String,
	sinceId As %Integer,
	prefix As %String = "hist.")
{
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT * FROM %IPM_General.History WHERE ID > ? ORDER BY ID", sinceId)
    if rs.%SQLCODE < 0 {
        do ..Obs(case, prefix_"sqlerror", rs.%SQLCODE_" "_rs.%Message)
        quit
    }
    set n = 0
    while rs.%Next() {
        set n = n + 1
        do ..ObsRow(case, prefix_"row"_n_".", rs)
    }
    do ..Obs(case, prefix_"rows", n)
}

/// Persisted history rows for one package with ID > sinceId.
ClassMethod HistFor(
	case As %String,
	package As %String,
	sinceId As %Integer,
	prefix As %String)
{
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT * FROM %IPM_General.History WHERE Package = ? AND ID > ? ORDER BY ID", package, sinceId)
    set n = 0
    while rs.%Next() {
        set n = n + 1
        do ..ObsRow(case, prefix_"row"_n_".", rs)
    }
    do ..Obs(case, prefix_"rows", n)
}

/// IRISTEMP HistoryTemp rows (instance-wide), optionally only those with ID > sinceTempId.
ClassMethod TempRows(
	case As %String,
	prefix As %String = "temp.",
	sinceTempId As %Integer = 0)
{
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT * FROM %IPM_General.HistoryTemp WHERE ID > ? ORDER BY ID", sinceTempId)
    if rs.%SQLCODE < 0 {
        do ..Obs(case, prefix_"sqlerror", rs.%SQLCODE_" "_rs.%Message)
        quit
    }
    set n = 0
    while rs.%Next() {
        set n = n + 1
        set p = prefix_"row"_n_"."
        do ..Obs(case, p_"ID", rs.%Get("ID"))
        do ..Obs(case, p_"Package", rs.%Get("Package"))
        do ..Obs(case, p_"Version", ..VersionFrom(rs))
        do ..Obs(case, p_"SuccessKind", ..StatusKind(rs.%Get("Success")))
        do ..Obs(case, p_"Committed", rs.%Get("Committed"))
        do ..Obs(case, p_"TimeEndSet", rs.%Get("TimeEnd") '= "")
        do ..Obs(case, p_"PersistedTwin", rs.%Get("PersistedTwin"))
        set phases = rs.%Get("Phases")
        set out = ""
        for i=1:1:$listlength(phases) {
            set ph = $listget($listget(phases, i), 1)
            set out = out_$select(out="":"",1:",")_$listget(ph,1)_":"_$select($listget(ph,3)="":"open",1:"ended")_":"_..StatusKind($listget(ph,4))
        }
        do ..Obs(case, p_"Phases", out)
    }
    do ..Obs(case, prefix_"rows", n)
}

ClassMethod MaxTempId() As %Integer
{
    quit +$order(^IRIS.Temp.HistoryTempD(""), -1)
}

/// Installed module state read from %IPM_Storage.ModuleItem by SQL.
ClassMethod ModState(
	case As %String,
	name As %String,
	prefix As %String = "mod.")
{
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT VersionString, DeveloperMode, Root FROM %IPM_Storage.ModuleItem WHERE Name = ?", name)
    if rs.%SQLCODE < 0 {
        do ..Obs(case, prefix_"sqlerror", rs.%SQLCODE_" "_rs.%Message)
        quit
    }
    if rs.%Next() {
        do ..Obs(case, prefix_"exists", 1)
        do ..Obs(case, prefix_"version", rs.%Get("VersionString"))
        do ..Obs(case, prefix_"dev", +rs.%Get("DeveloperMode"))
    } else {
        do ..Obs(case, prefix_"exists", 0)
    }
}

ClassMethod ClassState(
	case As %String,
	cls As %String,
	prefix As %String = "cls.")
{
    do ..Obs(case, prefix_"defined", ##class(%Dictionary.ClassDefinition).%ExistsId(cls))
    do ..Obs(case, prefix_"compiled", ##class(%Dictionary.CompiledClass).%ExistsId(cls))
}

ClassMethod Fixture(name As %String) As %String
{
    quit "/home/irisowner/zpm/.native/fixtures/"_name_"/1.0.0/"
}

/// Wait up to maxSeconds for a JOB to set ^IRIS.Temp.B2Signal(tag,"pid"). Returns 1 if it did.
ClassMethod WaitSignal(
	tag As %String,
	maxSeconds As %Integer = 300) As %Boolean
{
    for i=1:1:(maxSeconds*5) {
        if $data(^IRIS.Temp.B2Signal(tag, "pid")) { return 1 }
        hang 0.2
    }
    return 0
}

/// Wait up to maxSeconds for a process to disappear. Returns 1 if gone.
ClassMethod WaitGone(
	pid As %Integer,
	maxSeconds As %Integer = 120) As %Boolean
{
    for i=1:1:(maxSeconds*5) {
        if '$data(^$JOB(pid)) { return 1 }
        hang 0.2
    }
    return 0
}

}
