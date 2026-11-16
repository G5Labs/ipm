/// Test-only observer for iteration54. Not IPM code.
Class IPMCont.HelpReader
{

/// Command names defined in the pinned Commands XDATA (read on the host, not from IPM's structure).
Parameter COMMANDS = "help,quit,module-action,init,reload,sync,compile,test,package,verify,publish,update,makedeployed,unpublish,config,repo,load,import,run-from-file,install,reinstall,uninstall,exec,orphans,list-installed,list-dependents,show,default-modifiers,arrange,dependency-analyzer,version,generate,search,namespace,enable,unmap,module-version,information,history";

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

ClassMethod Clean(pLine As %String) As %String
{
    set m = ##class(%Regex.Matcher).%New("\x1B\[[0-9;]*m", pLine)
    quit $zstrip(m.ReplaceAll(""),"*C")
}

ClassMethod Run(pCase As %String, pCommand As %String, Output pLines) As %Boolean
{
    kill pLines
    do ##class(%IPM.Utils.Module).BeginCaptureOutput(.cookie)
    set sc = ##class(%IPM.Main).Shell(pCommand)
    do ##class(%IPM.Utils.Module).EndCaptureOutput(cookie,.raw)
    set pLines = 0
    for i=1:1:$get(raw) { set pLines = pLines + 1, pLines(pLines) = ..Clean($get(raw(i))) write !,"OUT|",pCase,"|",pLines(pLines) }
    if 'sc { write !,"ERR|",pCase,"|",$system.Status.GetErrorText(sc),! }
    quit $system.Status.IsOK(sc)
}

ClassMethod Has(ByRef pLines, pText As %String) As %Integer
{
    set n = 0
    for i=1:1:$get(pLines) { set:pLines(i)[pText n = n + 1 }
    quit n
}

/// Count defined commands that begin a line in the listing, and lines that begin with an undefined lowercase word.
ClassMethod Listing(pCase As %String, ByRef pLines)
{
    set listed = 0
    for k=1:1:$length(..#COMMANDS,",") {
        set c = $piece(..#COMMANDS,",",k), hit = 0
        for i=1:1:$get(pLines) {
            set w = $piece($piece($zstrip(pLines(i),"<W")," "),",")
            if (w = c) && ($extract(pLines(i)) '= " ") { set hit = 1 quit }
        }
        set listed = listed + hit
    }
    do ..Obs(pCase_".listed",listed)
}

ClassMethod Help(pCase As %String, pCommand As %String)
{
    set ok = ..Run(pCase,pCommand,.lines)
    do ..Obs(pCase_".ok",ok)
    do ..Obs(pCase_".warning",..Has(.lines,"does not exist so showing all available commands"))
    do ..Obs(pCase_".available",..Has(.lines,"Available commands:"))
    do ..Obs(pCase_".descriptions",..Has(.lines,"Description: "))
    do ..Obs(pCase_".flags",..Has(.lines,"Flags:"))
    do ..Obs(pCase_".arguments",..Has(.lines,"Arguments"))
    do ..Obs(pCase_".examples",..Has(.lines,"Examples"))
    do ..Obs(pCase_".only",..Has(.lines,"-only, -o"))
    do ..Obs(pCase_".dev",..Has(.lines,"-dev"))
    do ..Obs(pCase_".versiondesc",..Has(.lines,"Prints the currently-installed package manager"))
    do ..Obs(pCase_".moredetail",..Has(.lines,"For more detail, run:"))
    do ..Listing(pCase,.lines)
}

ClassMethod Info(pCase As %String)
{
    set ok = ..Run(pCase,"info",.lines)
    do ..Obs(pCase_".ok",ok)
    do ..Obs(pCase_".noregistry",..Has(.lines,"No registry configured"))
    do ..Obs(pCase_".currentregistry",..Has(.lines,"Current registry"))
}

ClassMethod All()
{
    do ..Help("h1","help")
    do ..Help("h2","help install")
    do ..Help("h3","help ver")
    do ..Help("h4","help nosuchcmd")
    do ..Help("h5","help 1")
    do ..Help("h6","help compile")
    do ..Help("h7","help -v")
    do ..Help("h8","help INSTALL")
    do ..Help("h9","help module-action")
    set ok = ..Run("v1","version",.lines)
    do ..Obs("v1.ok",ok)
    do ..Obs("v1.zpm",..Has(.lines,"(zpm)"))
    do ..Obs("v1.user",..Has(.lines,"USER"))
    do ..Obs("v1.installedin",..Has(.lines,"Installed In")+..Has(.lines,"installed in")+..Has(.lines,"mapped from"))
    do ..Obs("v1.snapshotupper",..Has(.lines,"0.10.11-SNAPSHOT"))
    do ..Info("i1")
    do ..Obs("r1.ok",..Run("r1","repo -fs -name fs54 -path /tmp/iter54/repo54/",.x))
    do ..Info("i2")
    do ..Obs("r2.ok",..Run("r2","repo -r -name reg54 -url http://127.0.0.1:9/",.x))
    set rs = ##class(%SQL.Statement).%ExecDirect(,"select count(*) n from %IPM_Repo_Remote.Definition")
    do rs.%Next()
    do ..Obs("r2.remotedefs",rs.%Get("n"))
    do ..Info("i3")
    write !,"NATIVE-END",!
}

}
