Include %IPM.Common
Class Iter25.DeveloperSources
{
ClassMethod Obs(k,v) { write !,"OBS|",k,"|",v,! }
ClassMethod Cmd(label,cmd)
{
 write !,"BEGIN|",label,!
 set sc=##class(%IPM.Main).Shell(cmd)
 write !,"END|",label,!
 do ..Obs(label,$$$ISOK(sc))
 if $$$ISERR(sc) {write !,"ERROR|",label,"|",$system.Status.GetErrorText(sc),!}
}
ClassMethod State(label)
{
 set m=##class(%IPM.Storage.Module).NameOpen("iter25-source")
 do ..Obs(label_"-present",$isobject(m))
 if $isobject(m) {
  do ..Obs(label_"-mode",+m.DeveloperMode),..Obs(label_"-version",m.VersionString="1.0.0"),..Obs(label_"-supplier",m.Repository="alpha")
  do ..Obs(label_"-root",##class(%File).NormalizeDirectory(m.Root)=##class(%File).NormalizeDirectory("/tmp/iter25/repo/iter25-source/"))
 }
 set code=$system.CLS.IsMthd("Iter25.Value","Who")>0
 do ..Obs(label_"-code",code)
 if code {do ..Obs(label_"-payload",##class(Iter25.Value).Who())}
 set map=$get(^Sources("CLS","Iter25.Value")),module=$get(^Sources("MODULE","iter25-source"))
 do ..Obs(label_"-class-map",$data(^Sources("CLS","Iter25.Value"))#2),..Obs(label_"-module-map",$data(^Sources("MODULE","iter25-source"))#2)
 do ..Obs(label_"-class-map-prefix",$ascii(map)=8),..Obs(label_"-module-map-prefix",$ascii(module)=8)
 write !,"MAP|",label,"|",$zconvert(map,"O","HEX"),"|",$zconvert(module,"O","HEX"),!
 set readerOK=0,path=""
 try {
  set reader=##class(%Studio.SourceControl.ISC).%New()
  set path=reader.ExternalName("Iter25.Value.CLS"),readerOK=1
 } catch ex {write !,"READER-ERROR|",label,"|",ex.DisplayString(),!}
 do ..Obs(label_"-reader-ok",readerOK)
 do ..Obs(label_"-reader-source",##class(%File).NormalizeFilename(path)="/tmp/iter25/repo/iter25-source/src/Iter25/Value.cls")
 do ..Obs(label_"-reader-exists",##class(%File).Exists(path))
 write !,"SOURCE-PATH|",label,"|",path,!
 do ..Cmd("show-"_label,"show iter25-source")
}
ClassMethod Run()
{
 do ..Cmd("repo-clear","repo -delete-all")
 do ..Cmd("repo-alpha","repo -fs -name alpha -depth 1 -path /tmp/iter25/repo/")
 do ..Cmd("install-normal","install alpha/iter25-source 1.0.0"),..State("normal")
 do ..Cmd("reinstall-dev","reinstall iter25-source -dev"),..State("dev")
 do ..Cmd("reinstall-normal-after-dev","reinstall iter25-source"),..State("refused")
 do ..Obs("workflow-complete",1),..Obs("final-tlevel",$tlevel)
}
}
