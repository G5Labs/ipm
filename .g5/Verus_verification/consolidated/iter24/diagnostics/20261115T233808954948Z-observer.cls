Include %IPM.Common
Class Iter24.SnapshotDependencies
{
ClassMethod Obs(k,v) { write !,"OBS|",k,"|",v,! }
ClassMethod Cmd(k,cmd)
{
 write !,"BEGIN|",k,!
 set sc=##class(%IPM.Main).Shell(cmd)
 write !,"END|",k,!
 do ..Obs(k,$$$ISOK(sc))
 if $$$ISERR(sc) {write !,"ERROR|",k,"|",$system.Status.GetErrorText(sc),!}
}
ClassMethod State(label,kind)
{
 set dep="iter24-"_kind,parent="iter24-parent-"_kind,ns="Iter24"_$select(kind="snap":"Snap",1:"Release")
 set m=##class(%IPM.Storage.Module).NameOpen(dep),p=##class(%IPM.Storage.Module).NameOpen(parent)
 do ..Obs(label_"-dep-present",$isobject(m)),..Obs(label_"-parent-present",$isobject(p))
 if $isobject(m) {
  do ..Obs(label_"-dep-version",m.VersionString=("1.0.0"_$select(kind="snap":"+snapshot",1:"")))
  do ..Obs(label_"-dep-snapshot",m.Version.IsSnapshot())
  do ..Obs(label_"-dep-build",m.Version.Build=$select(kind="snap":"snapshot",1:""))
  do ..Obs(label_"-dep-supplier",m.Repository="alpha")
  do ..Obs(label_"-dep-root",##class(%File).NormalizeDirectory(m.Root)=##class(%File).NormalizeDirectory("/tmp/iter24/repo/"_dep_"/"))
 }
 if $isobject(p) {do ..Obs(label_"-parent-version",p.VersionString="1.0.0")}
 set d=$system.CLS.IsMthd(ns_".Value","Who")>0,p=$system.CLS.IsMthd(ns_".Parent","Who")>0
 do ..Obs(label_"-dep-code",d),..Obs(label_"-parent-code",p)
 if d {do ..Obs(label_"-dep-payload",$classmethod(ns_".Value","Who"))}
 if p {do ..Obs(label_"-parent-payload",$classmethod(ns_".Parent","Who"))}
 do ..Cmd("show-dep-"_label,"show "_dep),..Cmd("show-parent-"_label,"show "_parent)
}
ClassMethod Replace(kind)
{
 set ns="Iter24"_$select(kind="snap":"Snap",1:"Release"),dest="/tmp/iter24/repo/iter24-"_kind_"/src/"_ns_"/Value.cls"
 set sc=##class(%File).CopyFile("/tmp/iter24/replacement/"_kind_"/Value.cls",dest)
 do ..Obs("copy-"_kind,sc)
 set s=##class(%Stream.FileCharacter).%New(),s.Filename=dest,text=s.Read()
 do ..Obs("physical-"_kind_"-two",$find(text,"quit 2")>0)
}
ClassMethod Graph(label,kind,force,permit=0)
{
 set p=##class(%IPM.Storage.Module).NameOpen("iter24-parent-"_kind),dep="iter24-"_kind
 set sc=p.BuildDependencyGraph(.g,,force,,,,,,,,permit)
 do ..Obs(label_"-status",$$$ISOK(sc)),..Obs(label_"-entry",$data(g(dep))>0)
 if $$$ISERR(sc) {write !,"GRAPH-ERROR|",label,"|",$system.Status.GetErrorText(sc),!}
 if $data(g(dep)) {
  do ..Obs(label_"-reuse",$listget(g(dep),2)="")
  do ..Obs(label_"-filesystem",$listget(g(dep),2)="filesystem")
  do ..Obs(label_"-version",$listget(g(dep),3)=("1.0.0"_$select(kind="snap":"+snapshot",1:"")))
  write !,"GRAPH|",label,"|",$listtostring(g(dep)),!
  do ##class(%IPM.Utils.Module).ConstructInvertedDependencyGraph(.g,.inverted)
  set refs=##class(%IPM.Utils.Module).GetFlatDependencyListFromInvertedDependencyGraph(.inverted),count=0
  for i=1:1:refs.Count() {
   set r=refs.GetAt(i)
   if r.Name=dep {
    set count=count+1
    do ..Obs(label_"-flat-reuse",r.ServerName=""),..Obs(label_"-flat-filesystem",r.ServerName="filesystem")
    do ..Obs(label_"-flat-version",r.VersionString=("1.0.0"_$select(kind="snap":"+snapshot",1:"")))
   }
  }
  do ..Obs(label_"-flat-count",count)
 }
}
ClassMethod Run()
{
 do ..Cmd("repo-clear","repo -delete-all")
 do ..Cmd("repo-alpha","repo -fs -name alpha -depth 1 -path /tmp/iter24/repo/")
 do ..Cmd("install-snap","install alpha/iter24-parent-snap 1.0.0"),..State("initial-snap","snap")
 do ..Cmd("install-release","install alpha/iter24-parent-release 1.0.0"),..State("initial-release","release")
 do ..Replace("snap"),..Replace("release")
 do ..State("changed-snap","snap"),..State("changed-release","release")
 do ..Graph("snap-no-force","snap",0),..Graph("snap-force","snap",1),..Graph("release-force","release",1)
 do ..Cmd("shallow-snap","reinstall iter24-parent-snap -shallow"),..State("shallow-snap","snap")
 do ..Cmd("default-snap","reinstall iter24-parent-snap"),..State("default-snap","snap")
 do ..Graph("snap-force-permit","snap",1,1)
 do ..Cmd("permit-snap","reinstall iter24-parent-snap -DPermitDowngrade=1"),..State("permit-snap","snap")
 do ..Cmd("default-release","reinstall iter24-parent-release"),..State("default-release","release")
 do ..Obs("final-tlevel",$tlevel)
}
}
