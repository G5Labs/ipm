Include %IPM.Common
Class Iter14.Protection
{
ClassMethod Obs(k,v) { write !,"OBS|",k,"|",v,! }
ClassMethod WriteFile(path,text)
{
 set f=##class(%Stream.FileCharacter).%New()
 $$$ThrowOnError(f.LinkToFile(path))
 $$$ThrowOnError(f.Write(text))
 $$$ThrowOnError(f.%Save())
}
ClassMethod Marker(dir)
{
 do ..WriteFile(dir_"marker.seed","synthetic, not a database")
 if '##class(%File).Rename(dir_"marker.seed",dir_"IRIS.DAT") {$$$ThrowStatus($$$ERROR($$$GeneralError,"Marker rename failed"))}
}
ClassMethod Content(path)
{
 if '##class(%File).Exists(path) {return 0}
 set f=##class(%Stream.FileCharacter).%New()
 $$$ThrowOnError(f.LinkToFile(path))
 return f.Read(100)="iter14 payload"
}
ClassMethod State(name,label)
{
 set dir="/tmp/iter14/targets/"_name_"/"
 do ..Obs(label_"-dir",##class(%File).DirectoryExists(dir))
 do ..Obs(label_"-payload",##class(%File).Exists(dir_"payload.txt")),..Obs(label_"-content",..Content(dir_"payload.txt"))
 do ..Obs(label_"-sentinel",##class(%File).Exists(dir_"sentinel.txt"))
 do ..Obs(label_"-marker",##class(%File).Exists(dir_"IRIS.DAT")),..Obs(label_"-nested",##class(%File).Exists(dir_"db/IRIS.DAT"))
 do ..Obs(label_"-protected",##class(%IPM.Utils.File).IsProtectedDirectory(dir))
 do ..Obs(label_"-row",##class(%IPM.Storage.Module).NameExists("iter14-"_name))
}
ClassMethod One(name,marker=0,change="",nested=0)
{
 set dir="/tmp/iter14/targets/"_name_"/"
 $$$ThrowOnError(##class(%IPM.Utils.File).CreateDirectoryChain(dir))
 do ..WriteFile(dir_"sentinel.txt","unrelated sentinel")
 if marker {do ..Marker(dir)}
 if nested {
  $$$ThrowOnError(##class(%IPM.Utils.File).CreateDirectoryChain(dir_"db/"))
  do ..Marker(dir_"db/")
 }
 do ..State(name,name_"-before")
 set sc=##class(%IPM.Main).Shell("load /tmp/iter14/modules/"_name_" -nodev")
 do ..Obs(name_"-load",$$$ISOK(sc))
 if $$$ISERR(sc) {write !,"RAW|",name,"|",$system.Status.GetErrorText(sc),!}
 do ..State(name,name_"-loaded")
 if change="add" {do ..Marker(dir)}
 if change="remove" {
  if '##class(%File).Rename(dir_"IRIS.DAT",dir_"marker.removed") {$$$ThrowStatus($$$ERROR($$$GeneralError,"Marker removal rename failed"))}
 }
 if change'="" {do ..State(name,name_"-changed")}
 set sc=##class(%IPM.Main).Shell("uninstall iter14-"_name)
 do ..Obs(name_"-uninstall",$$$ISOK(sc))
 if $$$ISERR(sc) {write !,"RAW|",name,"|",$system.Status.GetErrorText(sc),!}
 do ..State(name,name_"-cleaned")
}
ClassMethod Run()
{
 set install=$system.Util.InstallDirectory(),mgr=##class(%File).ManagerDirectory()
 do ..Obs("classify-install",##class(%IPM.Utils.File).IsProtectedDirectory(install))
 do ..Obs("classify-ancestor",##class(%IPM.Utils.File).IsProtectedDirectory(##class(%File).ParentDirectoryName(install)))
 do ..Obs("classify-child",##class(%IPM.Utils.File).IsProtectedDirectory(install_"iter14-unconfigured-child/"))
 do ..Obs("classify-normalized",##class(%IPM.Utils.File).IsProtectedDirectory(mgr_"/../csp"))
 do ..Obs("classify-case",##class(%IPM.Utils.File).IsProtectedDirectory($zconvert(mgr,"U")))
 set db=$piece(##class(%SYS.Namespace).GetGlobalDest($namespace),"^",2)
 set dirs=##class(%IPM.Utils.File).DatabaseDirectories(),ptr=0,found=0
 while $listnext(dirs,ptr,d) {if $zconvert(##class(%File).NormalizeDirectory(d),"L")=$zconvert(##class(%File).NormalizeDirectory(db),"L") {set found=1}}
 do ..Obs("database-listed",found)
 do ..One("ordinary"),..One("marked",1),..One("added",0,"add"),..One("removed",1,"remove"),..One("file",1),..One("nested",0,"",1)
 do ..Obs("final-tlevel",$tlevel)
}
}
