/// Observer for iteration74: prints OBS|key|int lines about %IPM.General.History rows.
Class IPMCov.Reader74
{

/// Highest History ID so far (0 if none).
ClassMethod Mark() As %Integer
{
    &sql(SELECT MAX(ID) INTO :max FROM %IPM_General.History)
    quit +$get(max)
}

ClassMethod Out(key As %String, val As %Integer)
{
    write !,"OBS|",key,"|",+val,!
}

ClassMethod ActionCode(a As %String) As %Integer
{
    quit $case(a, "load":1, "install":2, "uninstall":3, "update":4, :9)
}

ClassMethod PhaseCode(p As %String) As %Integer
{
    quit $case(p, "Initialize":1, "Reload":2, "Validate":3, "Compile":4, "Activate":5, "ApplyUpdateSteps":6, "Clean":7, :9)
}

/// Rows with ID > since: count, then per row (in ID order) action, package match, version, Success, Committed, end, phases.
ClassMethod Dump(
	case As %String,
	since As %Integer,
	pkg As %String = "",
	ver As %String = "")
{
    set n = 0
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT ID FROM %IPM_General.History WHERE ID > ? ORDER BY ID", since)
    while rs.%Next() {
        set n = n + 1
        set h = ##class(%IPM.General.History).%OpenId(rs.ID)
        set p = case_".r"_n
        do ..Out(p_".action", ..ActionCode(h.Action))
        do ..Out(p_".pkg", (h.Package = pkg))
        do ..Out(p_".pkgempty", (h.Package = ""))
        set v = $select($isobject(h.Version): h.Version.ToString(), 1: "")
        do ..Out(p_".verplaceholder", (v = "0.0.0-0"))
        do ..Out(p_".ver", (v = ver))
        do ..Out(p_".inprogress", (h.Success = 0))
        do ..Out(p_".ok", $$$ISOK(h.Success))
        do ..Out(p_".committed", h.Committed)
        do ..Out(p_".ended", (h.TimeEnd '= ""))
        do ..Out(p_".finalized", h.Finalized)
        do ..Out(p_".nph", h.Phases.Count())
        for i = 1:1:h.Phases.Count() {
            set ph = h.Phases.GetAt(i)
            do ..Out(p_".ph"_i, ..PhaseCode(ph.Name))
            do ..Out(p_".ph"_i_".ok", $$$ISOK(ph.Success))
            do ..Out(p_".ph"_i_".ended", (ph.TimeEnd '= ""))
        }
    }
    do ..Out(case_".rows", n)
}

ClassMethod Installed(
	case As %String,
	name As %String)
{
    do ..Out(case_".installed."_name, ##class(%IPM.Storage.Module).NameExists(name))
}

ClassMethod InstalledVersion(
	case As %String,
	name As %String,
	ver As %String)
{
    set m = ##class(%IPM.Storage.Module).NameOpen(name)
    do ..Out(case_".version."_name, $isobject(m) && (m.VersionString = ver))
}

ClassMethod ClassThere(
	case As %String,
	cls As %String)
{
    do ..Out(case_".class."_cls, ''$$$comClassDefined(cls))
}

ClassMethod FileThere(
	case As %String,
	path As %String)
{
    do ..Out(case_".file", ##class(%File).Exists(path))
}

/// Called from inside a running action (update step, Activate invoke, Clean processor): records what the newest
/// permanent History row and the newest HistoryTemp row look like while the action is still running.
ClassMethod Mid(
	key As %String,
	action As %String,
	pkg As %String)
{
    set id = ..Mark()
    &sql(SELECT Action, Package, TimeEnd, Success, Finalized INTO :a, :p, :te, :su, :fi FROM %IPM_General.History WHERE ID = :id)
    set ^IPMCov74("mid", key) = (SQLCODE = 0) && (a = action) && (p = pkg) && ($get(te) = "") && ($get(su) = 0) && '$get(fi)
    set h = ##class(%IPM.General.History).%OpenId(id)
    set ^IPMCov74("mid", key, "permph") = $select($isobject(h): h.Phases.Count(), 1: -1)
    &sql(SELECT MAX(ID) INTO :t FROM %IPM_General.HistoryTemp)
    set tmp = ##class(%IPM.General.HistoryTemp).%OpenId(+$get(t))
    set ^IPMCov74("mid", key, "tempph") = $select($isobject(tmp): tmp.Phases.Count(), 1: -1)
}

ClassMethod MidInstall()
{
    do ..Mid("install", "install", "h74a")
}

ClassMethod ShowMid(key As %String)
{
    do ..Out("mid."_key, $get(^IPMCov74("mid", key), -1))
    do ..Out("mid."_key_".permph", $get(^IPMCov74("mid", key, "permph"), -1))
    do ..Out("mid."_key_".tempph", $get(^IPMCov74("mid", key, "tempph"), -1))
}

}
