/// ORAS native checks NC-REPO-2, NC-REPO-9, NC-REPO-12 against the shared zot (http://oras:5000).
/// Every registry namespace and module name used here starts with nc-repo-.
Class Uplift.Repo.Oras Extends Uplift.Repo.Reg
{

/// Tags of <path> in zot, as a space-separated string; "<404>" if the repository is absent.
ClassMethod Tags(path As %String) As %String
{
    set req = ##class(%Net.HttpRequest).%New()
    set req.Server = "oras", req.Port = 5000
    set sc = req.Get("/v2/"_path_"/tags/list")
    if $$$ISERR(sc) { quit "<http error>" }
    if req.HttpResponse.StatusCode '= 200 { quit "<"_req.HttpResponse.StatusCode_">" }
    set obj = ##class(%DynamicObject).%FromJSON(req.HttpResponse.Data)
    set s = "", it = obj.tags.%GetIterator()
    while it.%GetNext(.k, .v) { set s = s_v_" " }
    quit $zstrip(s, ">W")
}

/// NC-REPO-2: what IPM still does with a disabled registry.
ClassMethod Repo2()
{
    do ..Reset()
    set path = "nc-repo-ns2/nc-repo-pub"
    set kv("repoFs") = $system.Status.IsOK(..Run("repo -fs -n nc-repo-f2 -path "_..#FIX_"fsrepo-extra/", .o))
    set kv("repoOras") = $system.Status.IsOK(..Run("repo -o -n nc-repo-o2 -url http://oras:5000 -ns nc-repo-ns2", .o))
    set kv("load") = $system.Status.IsOK(..Run("load "_..#FIX_"nc-repo-pub/", .o))
    set kv("publishEnabled") = $system.Status.IsOK(..Run("publish nc-repo-pub -r nc-repo-o2", .o))
    set kv("tagsAfterFirstPublish") = ..Tags(path)
    set kv("disable") = $system.Status.IsOK(..Run("repo -n nc-repo-o2 -disable", .o))
    set kv("enabledFlag") = ..Def("nc-repo-o2").Enabled
    set sc = ..Run("search", .out)
    do ..Status(sc, .kv, "search.")
    set kv("search.listsDisabledRepo") = ..Has(out, "nc-repo-o2")
    set kv("search.listsItsModule") = ..Has(out, "nc-repo-pub")
    set sc = ..Run("publish nc-repo-pub -r nc-repo-o2 -only", .out)
    do ..Status(sc, .kv, "publishDisabled.")
    set sc = ..Run("uninstall nc-repo-pub", .o)
    set sc = ..Run("install nc-repo-pub", .out)
    do ..Status(sc, .kv, "installDisabled.")
    set sc = ..Run("unpublish nc-repo-o2/nc-repo-pub all -force", .out)
    do ..Status(sc, .kv, "unpublishDisabled.")
    set kv("tagsAfterUnpublishDisabled") = ..Tags(path)
    do ..Emit("nc-repo-2-disabled-registry", .kv)
    // clean up the shared registry
    do ..Run("repo -n nc-repo-o2 -enable", .o)
    do ..Run("unpublish nc-repo-o2/nc-repo-pub all -force", .o)
}

/// NC-REPO-9: unpublish one version of a deployed module.
ClassMethod Repo9()
{
    do ..Reset()
    set path = "nc-repo-ns9/nc-repo-dpl"
    set kv("repo") = $system.Status.IsOK(..Run("repo -o -n nc-repo-o9 -url http://oras:5000 -ns nc-repo-ns9", .o))
    set kv("load") = $system.Status.IsOK(..Run("load "_..#FIX_"nc-repo-dpl/", .o))
    set sc = ..Run("publish nc-repo-dpl -r nc-repo-o9 -v", .o)
    do ..Status(sc, .kv, "publish.")
    set kv("tagsAfterPublish") = ..Tags(path)
    set sc = ..Run("unpublish nc-repo-o9/nc-repo-dpl 1.0.0 -force", .out)
    do ..Status(sc, .kv, "unpublishVersion.")
    set kv("tagsAfterUnpublishVersion") = ..Tags(path)
    set sc = ..Run("unpublish nc-repo-o9/nc-repo-dpl all -force", .out)
    do ..Status(sc, .kv, "unpublishAll.")
    set kv("tagsAfterUnpublishAll") = ..Tags(path)
    do ..Emit("nc-repo-9-unpublish-deployed-version", .kv)
}

/// NC-REPO-12: module published below the registry namespace; how list-modules names it.
ClassMethod Repo12()
{
    do ..Reset()
    set kv("repo") = $system.Status.IsOK(..Run("repo -o -n nc-repo-o12 -url http://oras:5000 -ns nc-repo-ns12/sub", .o))
    set kv("storedNs") = ..Def("nc-repo-o12").Namespace
    set kv("load") = $system.Status.IsOK(..Run("load "_..#FIX_"nc-repo-pub/", .o))
    set kv("publish") = $system.Status.IsOK(..Run("publish nc-repo-pub -r nc-repo-o12", .o))
    set kv("tags") = ..Tags("nc-repo-ns12/sub/nc-repo-pub")
    set kv("changeNs") = $system.Status.IsOK(..Run("repo -n nc-repo-o12 -ns nc-repo-ns12", .o))
    set sc = ..Run("repo -list-modules -n nc-repo-o12", .out)
    do ..Status(sc, .kv, "list.")
    set kv("list.showsSubPath") = ..Has(out, "sub/nc-repo-pub")
    set kv("list.showsNsPrefix") = ..Has(out, "nc-repo-ns12/")
    set kv("list.showsName") = ..Has(out, "nc-repo-pub")
    do ..Emit("nc-repo-12-nested-namespace-name", .kv)
    do ..Run("repo -n nc-repo-o12 -ns nc-repo-ns12/sub", .o)
    do ..Run("unpublish nc-repo-o12/nc-repo-pub all -force", .o)
    set kv2("tagsAfterCleanup") = ..Tags("nc-repo-ns12/sub/nc-repo-pub")
    do ..Emit("nc-repo-12-cleanup", .kv2)
}

ClassMethod RunAll()
{
    for m = "Repo2", "Repo9", "Repo12" {
        try {
            do $classmethod("Uplift.Repo.Oras", m)
        } catch e {
            write !, "OBS|error-oras-"_m_"|error="_..Safe(e.DisplayString())_";", !
        }
        if $tlevel { trollback }
    }
    do ..Reset()
}

}
