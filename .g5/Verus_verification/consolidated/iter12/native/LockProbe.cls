Include %IPM.Common
Class Iter12.LockProbe
{
ClassMethod Obs(k,v) { write !,"OBS|",k,"|",v,!}
ClassMethod Cmd(k,cmd) As %Boolean
{
 set sc=##class(%IPM.Main).Shell(cmd)
 do ..Obs(k,$$$ISOK(sc))
 if $$$ISERR(sc) {write !,"RAW|",k,"|",$system.Status.GetErrorText(sc),!}
 return $$$ISOK(sc)
}
ClassMethod Prepare() [ Language = python ]
{
    from pathlib import Path
    import shutil
    source=Path('/home/irisowner/zpm/tests/integration_tests/Test/PM/Integration/_data/lock-test')
    target=Path('/tmp/iter12/lockfile/product')
    shutil.copytree(source,target,dirs_exist_ok=True)
    for path in target.rglob('module-lock.json'):
        path.unlink()
}
ClassMethod Keys(object) As %String
{
 set names="",iterator=object.%GetIterator()
 while iterator.%GetNext(.key,.value) {set names=names_$select(names="":"",1:",")_key}
 return names
}
ClassMethod CountInstalledVersions(object) As %Boolean
{
 set matches=1,iterator=object.dependencies.%GetIterator()
 while iterator.%GetNext(.key,.value) {
  set module=##class(%IPM.Storage.Module).NameOpen(key,0,.sc)
  if '$$$ISOK(sc) {set matches=0 continue}
  if value.version'=module.VersionString {set matches=0}
  if value.repository'=module.Repository {set matches=0}
 }
 return matches
}
ClassMethod Read(label,path,expectedKeys) As %DynamicObject
{
 do ..Obs(label_"-file",##class(%File).Exists(path))
 set object=##class(%DynamicAbstractObject).%FromJSONFile(path)
 write !,"LOCKJSON|",label,"|",object.%ToJSON(),!
 do ..Obs(label_"-count",object.dependencies.%Size())
 do ..Obs(label_"-order",..Keys(object.dependencies)=expectedKeys)
 do ..Obs(label_"-versions",..CountInstalledVersions(object))
 do ..Obs(label_"-repos",object.repositories.%Size())
 do ..Obs(label_"-schema",object.lockFileVersion="1")
 return object
}
ClassMethod Flat(moduleName,label,expected,Output flat)
{
 set module=##class(%IPM.Storage.Module).NameOpen(moduleName,,.sc)
 $$$ThrowOnError(sc)
 $$$ThrowOnError(module.BuildDependencyGraph(.graph))
 do ##class(%IPM.Utils.Module).ConstructInvertedDependencyGraph(.graph,.inverted)
 set flat=##class(%IPM.Utils.Module).GetFlatDependencyListFromInvertedDependencyGraph(.inverted),names=""
 for i=1:1:flat.Count() {set names=names_$select(names="":"",1:",")_flat.GetAt(i).Name}
 write !,"FLAT|",label,"|",names,!
 do ..Obs(label_"-flat",names=expected),..Obs(label_"-flat-count",flat.Count())
}
ClassMethod Header(source,root) As %IPM.Storage.Module
{
 set header=##class(%IPM.Storage.Module).%New()
 set header.Name=source.Name,header.VersionString=source.VersionString,header.Repository=source.Repository,header.Root=root
 $$$ThrowOnError(##class(%IPM.Utils.File).CreateDirectoryChain(root))
 return header
}
ClassMethod Cursor()
{
 set graph("iter12-aa")=$listbuild("r","1.0.0",0,""),graph("iter12-aa","iter12-cc")=""
 set graph("iter12-bb")=$listbuild("r","1.0.0",0,""),graph("iter12-cc")=$listbuild("r","1.0.0",0,""),graph("iter12-dd")=$listbuild("r","1.0.0",0,"")
 set flat=##class(%IPM.Utils.Module).GetFlatDependencyListFromInvertedDependencyGraph(.graph),names=""
 for i=1:1:flat.Count() {set names=names_$select(names="":"",1:",")_flat.GetAt(i).Name}
 write !,"FLAT|lf-cursor|",names,!
 do ..Obs("lf-cursor",names="iter12-bb,iter12-cc,iter12-dd,iter12-aa")
 do ..Obs("lf-cursor-consumed",'$data(graph))
}
ClassMethod Credentials()
{
 set repo=##class(%IPM.Repo.Remote.Definition).%New(),repo.Name="iter12-credential",repo.URL="http://invalid.example/fixture",repo.Username="fixture-user",repo.Password="fixture-password",repo.Token="fixture-token"
 $$$ThrowOnError(repo.%Save())
 set header=##class(%IPM.Storage.Module).%New(),header.Name="iter12-credential",header.VersionString="1.0.0",header.Repository=repo.Name,header.Root="/tmp/iter12/lockfile/credential-output/"
 $$$ThrowOnError(##class(%IPM.Utils.File).CreateDirectoryChain(header.Root))
 set flat=##class(%Library.ListOfObjects).%New()
 do ##class(%IPM.General.LockFile).CreateLockFileForModule(header,.flat,.params)
 set object=##class(%DynamicAbstractObject).%FromJSONFile(header.Root_"module-lock.json"),value=object.repositories.%Get(repo.Name)
 write !,"LOCKJSON|lf-credential|",object.%ToJSON(),!
 do ..Obs("lf-credential-count",object.repositories.%Size())
 do ..Obs("lf-credential-url",value.url=repo.URL)
 do ..Obs("lf-credential-username",value.%IsDefined("username"))
 do ..Obs("lf-credential-password",value.%IsDefined("password"))
 do ..Obs("lf-credential-token",value.%IsDefined("token"))
 do ..Obs("lf-credential-marker",object.%ToJSON()["fixture-password")
 set failed=0
 try {set missing=##class(%IPM.General.LockFile).GetRepo("iter12-absent")} catch ex {set failed=1}
 do ..Obs("lf-missing-repo",failed)
}
ClassMethod Run()
{
 do ..Prepare()
 do ..Cmd("lf-delete-repos","repo -delete-all")
 do ..Cmd("lf-base-repo","repo -name lock-file-base -fs -path /tmp/iter12/lockfile/product/mods-base-cases/")
 do ..Cmd("lf-edge-repo","repo -name lock-file-edge -fs -path /tmp/iter12/lockfile/product/mods-edge-cases/")
 set k="lock-mod-k-common-transient",kNames="lock-mod-a-no-deps,lock-mod-b-no-deps,lock-mod-c-2-deps-0-transient,lock-mod-e-1-dep-0-transient"
 do ..Cmd("lf-k-install","install "_k_" -create-lockfile")
 set module=##class(%IPM.Storage.Module).NameOpen(k,,.sc)
 $$$ThrowOnError(sc)
 set object=..Read("lf-k",module.Root_"module-lock.json",kNames)
 do ..Obs("lf-k-requirement",object.dependencies.%Get("lock-mod-c-2-deps-0-transient").dependencies.%Get("lock-mod-a-no-deps")="^1.0.0")
 do ..Obs("lf-k-base-root",object.repositories.%Get("lock-file-base").root="/tmp/iter12/lockfile/product/mods-base-cases/")
 do ..Flat(k,"lf-k",kNames,.flat)
 set fake=##class(%IPM.Storage.QualifiedModuleInfo).%New(),fake.Name="lock-mod-a-no-deps",fake.VersionString="999.0.0",fake.ServerName="iter12-forged"
 $$$ThrowOnError(flat.Insert(fake))
 set header=..Header(module,"/tmp/iter12/lockfile/duplicate-output/")
 do ##class(%IPM.General.LockFile).CreateLockFileForModule(header,.flat,.params)
 set duplicate=..Read("lf-duplicate",header.Root_"module-lock.json",kNames)
 do ..Obs("lf-duplicate-installed",duplicate.dependencies.%Get(fake.Name).version="1.0.0")
 do ..Cmd("lf-l-install","install lock-mod-l-complex-deps -create-lockfile")
 set module=##class(%IPM.Storage.Module).NameOpen("lock-mod-l-complex-deps",,.sc)
 $$$ThrowOnError(sc)
 set lNames="lock-mod-a-no-deps,lock-mod-b-no-deps,lock-mod-c-2-deps-0-transient,lock-mod-d-1-dep-2-transient,lock-mod-e-1-dep-0-transient,lock-mod-f-2-deps-1-transient"
 set object=..Read("lf-l",module.Root_"module-lock.json",lNames)
 do ..Flat(module.Name,"lf-l",lNames,.flat)
 do ..Cmd("lf-order-repo","repo -name lock-file-order -fs -path /tmp/iter12/lockfile/order/")
 do ..Cmd("lf-order-install","install iter12-mm-top -create-lockfile")
 set module=##class(%IPM.Storage.Module).NameOpen("iter12-mm-top",,.sc)
 $$$ThrowOnError(sc)
 set object=..Read("lf-reverse",module.Root_"module-lock.json","iter12-aa-parent,iter12-zz-leaf")
 do ..Obs("lf-reverse-topological",..Keys(object.dependencies)="iter12-zz-leaf,iter12-aa-parent")
 do ..Flat(module.Name,"lf-reverse","iter12-zz-leaf,iter12-aa-parent",.flat)
 do ..Cursor(),..Credentials()
}
}
