Class Consolidated.Iter2.Probe
{
ClassMethod Registry()
{
    for name="g5iter2order","g5iter2sorted" {
        set tag=##class(%IPM.Repo.Oras.PackageService).GetLatestTag("http://127.0.0.1:18291",name,"","","","","basic")
        write !,"OBS TAG-",name," value=",tag,!
    }
    for name="g5iter2first","g5iter2skip","g5iter2match" {
        set ok=##class(%IPM.Repo.Oras.PackageService).VerifyModuleHashAndSize("http://127.0.0.1:18291",name_":v1","","","","","basic","aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",1)
        write !,"OBS HASH-",name," ok=",ok,!
    }
    for probe="accepted","rejected","unknown" {
        set e=##class(%IPM.Repo.Oras.HttpException).%New()
        set e.StatusCode=403
        do e.Classify("iter2","http://127.0.0.1:18291","pkg",1,probe)
        write !,"OBS AUTH-",probe," rejected=",e.CredentialsRejected," text=",e.Text,!
    }
}
ClassMethod Cache()
{
    set repo=##class(%IPM.Repo.Remote.Definition).ServerDefinitionKeyOpen("g5-iter2-cache")
    if '$isobject(repo) { set repo=##class(%IPM.Repo.Remote.Definition).%New() }
    set repo.Name="g5-iter2-cache",repo.URL="http://127.0.0.1:18291/",repo.Enabled=1
    set sc=repo.%Save()
    set manager=##class(%IPM.Repo.Remote.PublishManager).%Get()
    set client=manager.CheckServiceCache(repo.Name,.available)
    write !,"OBS CACHE-REMOTE before=",available," save=",$system.Status.IsOK(sc),!
    set repo.Enabled=0,sc=repo.%Save()
    set client=manager.CheckServiceCache(repo.Name,.available)
    write !,"OBS CACHE-REMOTE disabled=",available," save=",$system.Status.IsOK(sc),!
    set id=repo.%Id(),repo="",sc=##class(%IPM.Repo.Remote.Definition).%DeleteId(id)
    set repo=##class(%IPM.Repo.Oras.Definition).ServerDefinitionKeyOpen("g5-iter2-orascache")
    if '$isobject(repo) { set repo=##class(%IPM.Repo.Oras.Definition).%New() }
    set repo.Name="g5-iter2-orascache",repo.URL="http://127.0.0.1:18291",repo.Enabled=1
    set sc=repo.%Save()
    set manager=##class(%IPM.Repo.Oras.PublishManager).%Get()
    set client=manager.CheckServiceCache(repo.Name,.available)
    write !,"OBS CACHE-ORAS before=",available," save=",$system.Status.IsOK(sc),!
    set repo.Enabled=0,sc=repo.%Save()
    set client=manager.CheckServiceCache(repo.Name,.available)
    write !,"OBS CACHE-ORAS disabled=",available," save=",$system.Status.IsOK(sc),!
    set id=repo.%Id(),repo="",sc=##class(%IPM.Repo.Oras.Definition).%DeleteId(id)
}
ClassMethod Readers()
{
    set idx=-882142
    kill ^UnitTest.Result(idx)
    set ^UnitTest.Result(idx,"Iter2Suite")=$lb(1,0)
    set ^UnitTest.Result(idx,"Iter2Suite","Case")=$lb(1,0)
    set ^UnitTest.Result(idx,"Iter2Suite","Case","TestMethod")=$lb(0,0)
    set ^UnitTest.Result(idx,"Iter2Suite","Case","TestMethod",1)=$lb(1,"AssertTrue","passed assertion","")
    set tree=##class(%IPM.Test.Abstract).BuildResultTree(idx)
    set summary=##class(%IPM.Test.Abstract).GetSummary(idx,tree)
    write !,"OBS REPORT-METHOD methods=",summary.methods.total," failed=",summary.methods.failed," assertions=",summary.assertions.failed,!
    set ^UnitTest.Result(idx,"Iter2Suite","Case")=$lb(0,0,"HookFailure","iter2")
    set ^UnitTest.Result(idx,"Iter2Suite","Case","TestMethod")=$lb(1,0)
    set tree=##class(%IPM.Test.Abstract).BuildResultTree(idx)
    set summary=##class(%IPM.Test.Abstract).GetSummary(idx,tree)
    write !,"OBS REPORT-CASE methods=",summary.methods.total," failed=",summary.methods.failed," assertions=",summary.assertions.failed,!
    set sc=##class(%IPM.Test.JsonOutput).ToFile("/tmp/g5-iter2/report.json",idx)
    set s=##class(%Stream.FileCharacter).%New()
    do s.LinkToFile("/tmp/g5-iter2/report.json")
    set obj={}.%FromJSON(s)
    write !,"OBS REPORT-FILE status=",$system.Status.IsOK(sc)," suites=",obj.%IsDefined("suites")," summary=",obj.%IsDefined("summary"),!
    kill ^UnitTest.Result(idx)
    set s=##class(%Stream.FileCharacter).%New()
    do s.LinkToFile("/tmp/g5-iter2/requirements.txt")
    set lines=##class(%IPM.General.ModuleSummary).ParseRequirements(s)
    write !,"OBS REQUIREMENTS count=",lines.%Size()," line=",lines.%Get(0),!
}
ClassMethod Environment()
{
    set env=##class(%IPM.General.EnvironmentConfig).%Get()
    for pair="objects","primitives" {
        do env.Clear()
        do env.Load("/tmp/g5-iter2/"_pair_"-a.json")
        do env.Load("/tmp/g5-iter2/"_pair_"-b.json")
        set obj=env.GetArg("pkg","arr")
        write !,"OBS ENV-",pair," value=",obj.%ToJSON(),!
    }
    do env.Clear()
    set p("a")="${b}",p("b")="${a}"
    do ##class(%IPM.Storage.ModuleSetting.Default).ResolvePlaceholders(.p)
    write !,"OBS CYCLE a=",p("a")," b=",p("b"),!
    kill p
    set p("a")="${b}",p("b")="ok"
    do ##class(%IPM.Storage.ModuleSetting.Default).ResolvePlaceholders(.p)
    write !,"OBS CHAIN a=",p("a")," b=",p("b"),!
}
}
