Include %IPM.Common
Class Iter19.Duplicates
{
ClassMethod Obs(k,v) { write !,"OBS|",k,"|",v,! }
ClassMethod Cmd(label,command)
{
 write !,"BEGIN|",label,!
 set sc=##class(%IPM.Main).Shell(command)
 write !,"END|",label,!
 do ..Obs(label,$$$ISOK(sc))
 if $$$ISERR(sc) {write !,"STATUS|",label,"|",$system.Status.GetErrorText(sc),!}
}
ClassMethod Text(s)
{
 do s.Rewind()
 set text=""
 while 's.AtEnd {set text=text_s.Read(30000)}
 return text
}
ClassMethod Marker(text)
{
 if text["source-A" {return 1}
 if text["source-B" {return 2}
 return 0
}
ClassMethod PhysicalMarker(directory)
{
 set f=##class(%Stream.FileCharacter).%New()
 $$$ThrowOnError(f.LinkToFile(directory_"module.xml"))
 return ..Marker(..Text(f))
}
ClassMethod Inspect(label)
{
 set d=##class(%IPM.Repo.Definition).ServerDefinitionKeyOpen(label)
 do ..Obs(label_"-definition",$isobject(d))
 set root="/tmp/iter19/repos/"_label_"/"
 do ..Obs(label_"-root",d.Root=root),..Obs(label_"-depth",d.Depth)
 set sc=d.ScanDirectory(root,.files,,1)
 do ..Obs(label_"-scan-ok",$$$ISOK(sc)),..Obs(label_"-scan-count",+$get(files))
 set i="",first="",firstMarker=0
 for {
  set i=$order(files(i)) quit:i=""
  set sub=files(i,"sub"),directory=##class(%File).NormalizeDirectory(root_sub),marker=..PhysicalMarker(directory)
  if first="" {set first=sub,firstMarker=marker}
  do ..Obs(label_"-scan-"_i_"-marker",marker)
  write !,"SCAN|",label,"|",i,"|",sub,"|",files(i),"|",marker,!
 }
 set rs=##class(%SQL.Statement).%ExecDirect(,"SELECT ID,SubDirectory,Name,VersionString FROM %IPM_Repo_Filesystem.Cache WHERE Root=? AND Name=? AND VersionString=? ORDER BY ID",root,"iter19-dupe","1.0.0")
 $$$ThrowSQLIfError(rs.%SQLCODE,rs.%Message)
 set count=0,id="",cacheDir="",cacheMarker=0
 while rs.%Next() {
  set count=count+1,id=rs.%Get("ID"),cacheDir=rs.%Get("SubDirectory")
  set row=##class(%IPM.Repo.Filesystem.Cache).%OpenId(id)
  set cacheMarker=..Marker(..Text(row.Manifest))
  do ..Obs(label_"-cache-first-directory",cacheDir=first),..Obs(label_"-cache-marker",cacheMarker)
  do ..Obs(label_"-cache-name",row.Name="iter19-dupe"),..Obs(label_"-cache-version",row.VersionString="1.0.0")
  write !,"CACHE|",label,"|",id,"|",cacheDir,"|",cacheMarker,!
 }
 do ..Obs(label_"-cache-count",count),..Obs(label_"-first-marker",firstMarker)
 set client=d.GetPackageService(),ref=##class(%IPM.Storage.ModuleInfo).%New()
 set ref.Name="iter19-dupe",ref.VersionString="1.0.0"
 set manifest=client.GetModuleManifest(ref),serviceMarker=..Marker(..Text(manifest)),directory=client.GetModuleDirectory(ref)
 do ..Obs(label_"-service-marker",serviceMarker)
 do ..Obs(label_"-service-directory-cache",directory=##class(%File).NormalizeDirectory(root_cacheDir))
 do ..Obs(label_"-service-directory-marker",..PhysicalMarker(directory))
 do ..Obs(label_"-cache-service-agree",cacheMarker=serviceMarker),..Obs(label_"-cache-first-marker",cacheMarker=firstMarker)
 write !,"DIRECTORY|",label,"|",directory,!
 do ..Cmd("install-"_label,"install "_label_"/iter19-dupe 1.0.0")
 set m=##class(%IPM.Storage.Module).NameOpen("iter19-dupe")
 do ..Obs(label_"-installed",$isobject(m))
 do ..Obs(label_"-installed-root",##class(%File).NormalizeDirectory(m.Root)=directory)
 do ..Obs(label_"-installed-supplier",m.Repository=label),..Obs(label_"-installed-version",m.VersionString="1.0.0")
 do ..Obs(label_"-installed-marker",..Marker(m.Description))
 set available=$system.CLS.IsMthd("Iter19Dup.Payload","Who")>0
 do ..Obs(label_"-code",available)
 if available {do ..Obs(label_"-payload",$classmethod("Iter19Dup.Payload","Who"))}
 do ..Cmd("show-"_label,"show iter19-dupe"),..Cmd("list-"_label,"list -repository "_label)
}
ClassMethod Run()
{
 do ..Cmd("repo-clear","repo -delete-all")
 for label="single","ab","ba" {
  do ..Cmd("repo-"_label,"repo -fs -name "_label_" -depth 1 -path /tmp/iter19/repos/"_label)
  do ..Inspect(label)
 }
 do ..Obs("final-tlevel",$tlevel)
}
}
