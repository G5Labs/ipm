Include %IPM.Common

/// Integration tests for ORAS repos configured with a Docker Content Trust server (repo -trust-server).
/// Modules are published with the ordinary publish command, then signed with the notary client against the
/// Notary server in tests/notary, as an external promotion process would.
/// Each test uses a registry namespace of its own that is unique to the run. The trust server keeps a collection's
/// root keys and trust data for its lifetime, so a reused collection name would meet those of an earlier run.
Class Test.PM.Integration.OrasContentTrust Extends Test.PM.Integration.Base
{

Parameter OrasURL = "http://oras:5000";

Parameter TrustServer = "https://notary:4443";

/// Registry host that collection names start with, as IRIS reaches the registry.
Parameter RegistryHost = "oras:5000";

Parameter RepoName = "signed";

Parameter AppName = "oras-trust-app";

Parameter DepName = "oras-trust-dep";

Parameter DepVersion = "1.0.0+build.1";

/// Registry tag of DepVersion.
Parameter DepTag = "1.0.0_build.1";

/// Platform guaranteed not to match any real IRIS instance.
Parameter IncompatiblePlatform = "9999.1";

/// Registry namespace of the current test.
Property Namespace As %String(MAXLEN = "");

/// Notary trust directory holding the keys of the external promotion process.
Property SignerDirectory As %String(MAXLEN = "");

/// Set when the notary client cannot run here, which fails the tests.
Property NotaryMissing As %Boolean [ InitialExpression = 0 ];

/// Collections(gun) = trust directory of the signer that created collection gun in the current test.
Property Collections As %String [ MultiDimensional ];

/// Published(repo, package) for each package the current test published.
Property Published As %String [ MultiDimensional ];

Method OnBeforeAllTests() As %Status
{
    set ..NotaryMissing = '..CanRunNotary()
    set ..SignerDirectory = ##class(%IPM.Utils.File).CreateTempDirectory()
    return $$$OK
}

Method OnAfterAllTests() As %Status
{
    do ##class(%IPM.Main).Shell("repo -delete-all")
    do ##class(%IPM.Utils.File).RemoveDirectoryTree(..SignerDirectory)
    return $$$OK
}

Method OnBeforeOneTest(testName As %String) As %Status
{
    set ..Namespace = "content-trust/" _ $zconvert(testName, "L") _ "-" _ $job _ "-" _ $translate($horolog, ",", "-")
    kill ..Collections, ..Published, ^OrasTrustApp
    for module = ..#AppName, ..#DepName {
        do ..RemoveExtracted(module)
    }
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("repo -delete-all"), "Deleted repos")
    set sc = ##class(%IPM.Main).Shell("repo -o -name " _ ..#RepoName _ " -url " _ ..#OrasURL _ " -namespace " _ ..Namespace _ " -trust-server " _ ..#TrustServer _ " -publish 1")
    do $$$AssertStatusOK(sc, "Configured repo with a trust server")
    return $$$OK
}

Method OnAfterOneTest(testName As %String) As %Status
{
    for module = ..#AppName, ..#DepName, "oras-trust-other", "oras-trust-impostor", "oras-auth", "oras-dep-consumer-name", "default-name", "platform-filter-test" {
        if ##class(%IPM.Storage.Module).NameExists(module) {
            do ##class(%IPM.Main).Shell("uninstall " _ module)
        }
    }
    set repo = ""
    for {
        set repo = $order(..Published(repo))
        quit:repo=""
        set package = ""
        for {
            set package = $order(..Published(repo, package))
            quit:package=""
            do ##class(%IPM.Main).Shell("unpublish " _ repo _ "/" _ package _ " all -force")
        }
    }
    do ##class(%IPM.Main).Shell("repo -delete-all")
    do ##class(%IPM.Main).Shell("config delete NotaryPath")
    set gun = ""
    for {
        set gun = $order(..Collections(gun), 1, signer)
        quit:gun=""
        do ..RunNotary(signer, .output, "delete", gun, "--remote")
    }
    set rs = ##class(%SQL.Statement).%ExecDirect(, "DELETE FROM %IPM_Repo_Oras.TrustedCollection WHERE GUN %STARTSWITH ?", ..#RegistryHost _ "/")
    do $$$AssertTrue(rs.%SQLCODE >= 0, "Removed trust data pinned by the test")
    kill ^OrasTrustApp
    return $$$OK
}

// ===== Configuration =====

/// The trust server is configured with -trust-server, shown with the repo, given to its package service,
/// kept where the rest of the definition is copied or exported, and cleared with an empty value.
Method TestConfigureTrustServer()
{
    set sc = ##class(%IPM.Main).Shell("repo -o -name cfg -url " _ ..#OrasURL _ " -namespace " _ ..Namespace _ "-cfg -trust-server https://Notary:4443/")
    do $$$AssertStatusOK(sc, "Configured a repo with a trust server")
    set repo = ##class(%IPM.Repo.Oras.Definition).ServerDefinitionKeyOpen("cfg")
    do $$$AssertEquals(repo.TrustServer, ..#TrustServer, "Trust server URL is stored canonically")
    do $$$AssertEquals(repo.GetPackageService().TrustServer, ..#TrustServer, "Package service verifies against the trust server")
    do $$$AssertEquals(repo.GetPublishService().TrustServer, "", "Publishing does not involve the trust server")

    set cookie = ""
    do ##class(%IPM.Utils.Module).BeginCaptureOutput(.cookie)
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("repo -list"))
    do ##class(%IPM.Utils.Module).EndCaptureOutput(cookie, .lines)
    do $$$AssertTrue(..FindStringInMultiDimArray("Trust Server:", .lines), "repo -list shows the trust server")

    // repo -copy-from saves a deep clone of each definition
    do $$$AssertEquals(repo.%ConstructClone(1).TrustServer, ..#TrustServer, "A copied definition keeps the trust server")
    do $$$AssertStatusOK(repo.%JSONExportToString(.json, "LockFileMapping"))
    do $$$AssertEquals({}.%FromJSON(json).trustServer, ..#TrustServer, "Lock files record the trust server")

    do $$$AssertStatusOK(##class(%IPM.Main).Shell("repo -n cfg -disable"), "Changed another setting")
    do $$$AssertStatusOK(repo.%Reload())
    do $$$AssertEquals(repo.TrustServer, ..#TrustServer, "Other settings leave the trust server alone")

    do $$$AssertStatusNotOK(##class(%IPM.Main).Shell("repo -n cfg -trust-server ""not a url"""), "An invalid trust server URL is refused")
    do ..AssertRefused(##class(%IPM.Main).Shell("repo -n cfg -trust-server http://notary:4443"), "Trust server URL 'http://notary:4443' must use https.")
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("repo -n cfg -trust-server notary.example.com"), "A trust server without a scheme uses https")
    do $$$AssertStatusOK(repo.%Reload())
    do $$$AssertEquals(repo.TrustServer, "https://notary.example.com")
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("repo -n cfg -trust-server """""), "Cleared the trust server")
    do $$$AssertStatusOK(repo.%Reload())
    do $$$AssertEquals(repo.TrustServer, "", "Trust server cleared")
    do $$$AssertEquals(repo.GetPackageService().TrustServer, "", "Package service no longer verifies")
}

/// Repo types that cannot verify refuse a trust server instead of ignoring it.
Method TestTrustServerRefusedForOtherRepoTypes()
{
    set sc = ##class(%IPM.Main).Shell("repo -r -name cfg-remote -url http://registry:52773/registry/ -trust-server " _ ..#TrustServer)
    do ..AssertRefused(sc, "-trust-server applies only to ORAS repositories, not to repository 'cfg-remote' of type Remote Repository.")
    do $$$AssertNotTrue(##class(%IPM.Repo.Definition).ServerDefinitionKeyExists("cfg-remote"), "No remote repo was created")

    set path = ##class(%IPM.Utils.File).CreateTempDirectory()
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("repo -fs -name cfg-fs -path " _ path), "Configured a filesystem repo")
    set sc = ##class(%IPM.Main).Shell("repo -n cfg-fs -trust-server " _ ..#TrustServer)
    do ..AssertRefused(sc, "not to repository 'cfg-fs' of type Filesystem.")
    do ##class(%IPM.Utils.File).RemoveDirectoryTree(path)
}

/// A repo without a trust server, including one saved before the setting existed, installs unsigned modules as before.
Method TestRepoWithoutTrustServerUnchanged()
{
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("repo -n " _ ..#RepoName _ " -trust-server """""), "Cleared the trust server")
    set repo = ##class(%IPM.Repo.Definition).ServerDefinitionKeyOpen(..#RepoName)
    set node = $name(^IPM.Repo.DefinitionD(repo.%Id(), "OrasRepoDefinition"))
    do $$$AssertEquals($listlength(@node), 7, "Stored definition has the trust server slot")
    // As saved by IPM versions without the setting
    set @node = $list(@node, 1, 6)

    do ..PublishAppAndDep()
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("install " _ ..#RepoName _ "/" _ ..#AppName), "Installed unsigned modules")
    do $$$AssertEquals(##class(OrasTrustApp.Info).Describe(), "app 1.0.0 with dep " _ ..#DepVersion)
}

// ===== Signed installs =====

/// Modules published as usual and then signed externally install, dependency included, and each collection's trust data is pinned.
Method TestSignedModuleInstalls()
{
    if '..NotaryRuns() {
        return
    }
    do ..PublishAppAndDep()
    do ..Sign(..#AppName, "1.0.0")
    do ..Sign(..#DepName, ..#DepTag)

    set sc = ##class(%IPM.Main).Shell("install " _ ..#RepoName _ "/" _ ..#AppName)
    do $$$AssertStatusOK(sc, "Installed signed module and its signed dependency")
    do ..AssertVersion(..#AppName, "1.0.0")
    do ..AssertVersion(..#DepName, ..#DepVersion)
    do $$$AssertEquals(##class(OrasTrustApp.Info).Describe(), "app 1.0.0 with dep " _ ..#DepVersion)
    do $$$AssertTrue($data(^OrasTrustApp("configured", "1.0.0")), "Module lifecycle ran")
    do $$$AssertTrue(##class(%File).DirectoryExists(..ExtractionDirectory(..#AppName, "1.0.0")), "Unpacked where the refusal tests look")
    for package = ..#AppName, ..#DepName {
        do $$$AssertTrue(##class(%IPM.Repo.Oras.TrustedCollection).GUNIndexExists(..GUN(package)), "Pinned trust data for " _ package)
    }
}

/// Docker Content Trust takes a tag from the targets/releases delegation or the targets role, and no other delegation.
Method TestOnlyReleasesOrTargetsRoleCounts()
{
    if '..NotaryRuns() {
        return
    }
    do ..PublishAppAndDep()
    do ..Sign(..#DepName, ..#DepTag)
    do ..AddDelegation(..#AppName, "targets/qa")
    do ..Sign(..#AppName, "1.0.0", , "targets/qa")

    set sc = ##class(%IPM.Main).Shell("install " _ ..#RepoName _ "/" _ ..#AppName)
    do ..AssertRefused(sc, "has no signed trust data for '" _ ..GUN(..#AppName) _ ":1.0.0'")
    do ..AssertNotInstalled(..#AppName)

    do ..AddDelegation(..#AppName, "targets/releases")
    do ..Sign(..#AppName, "1.0.0", , "targets/releases")
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("install " _ ..#RepoName _ "/" _ ..#AppName), "Installed module signed by targets/releases")
    do ..AssertVersion(..#AppName, "1.0.0")
}

/// A deployed artifact verifies under its platform tag, and the platform filter still picks among signed artifacts.
Method TestPlatformArtifacts()
{
    if '..NotaryRuns() {
        return
    }
    set module = "platform-filter-test"
    set platformTag = "1.0.0" _ $$$OrasTagPlatformSeparator _ $$$CurrentPlatformVersion
    do ..LoadAndPackage(..GetModuleDir("platform-filter", "test", "v1"), module)
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("publish " _ module _ " -r " _ ..#RepoName _ " -only"), "Published 1.0.0 for this platform")
    set ..Published(..#RepoName, module) = ""
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("load -nodev " _ ..GetModuleDir("platform-filter", "test", "v2")), "Loaded 2.0.0")
    do ..PublishForPlatform(module, ..#IncompatiblePlatform)
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("uninstall " _ module))
    do ..Sign(module, platformTag)
    do ..Sign(module, "2.0.0" _ $$$OrasTagPlatformSeparator _ ..#IncompatiblePlatform)

    do $$$AssertStatusOK(##class(%IPM.Main).Shell("install " _ ..#RepoName _ "/" _ module), "Installed the compatible signed artifact")
    do ..AssertVersion(module, "1.0.0")
    do $$$AssertEquals($classmethod("PlatformFilterTest.Public", "Version"), "1.0.0", "1.0.0 sources were installed")

    set sc = ##class(%IPM.Main).Shell("install " _ ..#RepoName _ "/" _ module _ " 2.0.0")
    do ..AssertRefused(sc, "is not available for platform version")
    do ..AssertVersion(module, "1.0.0")
}

/// A module published under its external name in a namespace verifies under that name and installs as a dependency under its own name.
Method TestDependencyByExternalNameInNamespace()
{
    if '..NotaryRuns() {
        return
    }
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("load " _ ..GetModuleDir("publish-external-name", "0.0.1")), "Loaded the dependency")
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("publish default-name -r " _ ..#RepoName _ " -only -use-external-name"), "Published under its external name")
    set ..Published(..#RepoName, "external-name") = ""
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("uninstall default-name"))

    // The repo offers the module under another name only once its signed module.xml confirms the name
    set consumer = ..GetModuleDir("oras-dep-name", "consumer-name")
    set cookie = ""
    do ##class(%IPM.Utils.Module).BeginCaptureOutput(.cookie)
    set sc = ##class(%IPM.Main).Shell("load " _ consumer)
    do ##class(%IPM.Utils.Module).EndCaptureOutput(cookie, .lines)
    do ..AssertRefused(sc, "Could not find satisfactory version of default-name")
    do $$$AssertTrue(..FindStringInMultiDimArray("Not offering 'external-name' 0.0.1 from repo '" _ ..#RepoName _ "' as 'default-name'", .lines), "Warns why the module is not offered")
    do $$$AssertTrue(..FindStringInMultiDimArray(..GUN("external-name"), .lines), "The warning names the unverified collection")
    do ..AssertNotInstalled("default-name")

    do ..Sign("external-name", "0.0.1")
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("load " _ consumer), "Loaded the consumer with its signed dependency")
    do ..AssertVersion("default-name", "0.0.1")
}

/// A deployed module offered under another name is checked against the signed module.xml of this platform's artifact,
/// which is the one that installs, even when another platform's artifact is listed first and is not signed.
Method TestExternalNameCheckedOnPlatformArtifact()
{
    if '..NotaryRuns() {
        return
    }
    set package = "external-name"
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("load -nodev " _ ..GetModuleDir("publish-external-name", "0.0.1")), "Loaded the dependency")
    // The registry lists this platform's tag after that of the lower platform version
    do ..PublishForPlatform("default-name", "1000.1", package)
    do ..PublishForPlatform("default-name", $$$CurrentPlatformVersion, package)
    set ..Published(..#RepoName, package) = ""
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("uninstall default-name"))
    do ..Sign(package, "0.0.1" _ $$$OrasTagPlatformSeparator _ $$$CurrentPlatformVersion)

    do $$$AssertStatusOK(##class(%IPM.Main).Shell("load " _ ..GetModuleDir("oras-dep-name", "consumer-name")), "Loaded the consumer with this platform's signed artifact")
    do ..AssertVersion("default-name", "0.0.1")
}

/// Unsigned listing data decides whether install retrieves the untagged or this platform's artifact of a version.
/// A module offered under another name is checked against the signed module.xml of the one it decides on.
Method TestExternalNameCheckedOnInstalledArtifact()
{
    if '..NotaryRuns() {
        return
    }
    set package = "external-name"
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("load " _ ..GetModuleDir("publish-external-name", "0.0.1")), "Loaded the dependency")
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("publish default-name -r " _ ..#RepoName _ " -only -use-external-name"), "Published under its external name")
    set ..Published(..#RepoName, package) = ""
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("uninstall default-name"))
    // The collection's artifact of the same version for this platform is another module
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("load -nodev " _ ..GetModuleDir("oras-content-trust", "impostor")), "Loaded the other module")
    do ..PublishForPlatform("oras-trust-impostor", $$$CurrentPlatformVersion, package)
    do ..Uninstall("oras-trust-impostor")
    do ..Sign(package, "0.0.1")
    do ..Sign(package, "0.0.1" _ $$$OrasTagPlatformSeparator _ $$$CurrentPlatformVersion)

    // The listing reads the untagged artifact's metadata. Marked deployed, install retrieves this platform's artifact.
    set original = ..GetManifest(package, "0.0.1")
    set manifest = {}.%FromJSON(original)
    do manifest.annotations.%Set("com.intersystems.ipm.deployed", "1")
    do ..PutManifest(package, "0.0.1", manifest.%ToJSON())

    set consumer = ..GetModuleDir("oras-dep-name", "consumer-name")
    do ..AssertRefused(##class(%IPM.Main).Shell("load " _ consumer), "Could not find satisfactory version of default-name")
    do ..AssertNotInstalled("oras-trust-impostor")
    do ..AssertNotInstalled("default-name")

    do ..PutManifest(package, "0.0.1", original)
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("load " _ consumer), "Loaded the consumer with the untagged artifact")
    do ..AssertVersion("default-name", "0.0.1")
    do ..AssertNotInstalled("oras-trust-impostor")
}

/// A signed module.xml annotation with line breaks around it is read as without a trust server,
/// both to resolve a dependency and to offer a module under its external name.
Method TestModuleXMLWhitespaceTrimmed()
{
    if '..NotaryRuns() {
        return
    }
    do ..PublishAppAndDep()
    do ..PadModuleXML(..#DepName, ..#DepTag)
    do ..Sign(..#AppName, "1.0.0")
    do ..Sign(..#DepName, ..#DepTag)
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("install " _ ..#RepoName _ "/" _ ..#AppName), "Installed with a dependency whose module.xml is padded")
    do $$$AssertEquals(##class(OrasTrustApp.Info).Describe(), "app 1.0.0 with dep " _ ..#DepVersion)

    do $$$AssertStatusOK(##class(%IPM.Main).Shell("load " _ ..GetModuleDir("publish-external-name", "0.0.1")), "Loaded the dependency")
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("publish default-name -r " _ ..#RepoName _ " -only -use-external-name"), "Published under its external name")
    set ..Published(..#RepoName, "external-name") = ""
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("uninstall default-name"))
    do ..PadModuleXML("external-name", "0.0.1")
    do ..Sign("external-name", "0.0.1")
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("load " _ ..GetModuleDir("oras-dep-name", "consumer-name")), "Loaded the consumer with its padded external-name dependency")
    do ..AssertVersion("default-name", "0.0.1")
}

/// Basic authentication to the registry works alongside the trust server, and refusals show no password.
Method TestRegistryCredentials()
{
    if '..NotaryRuns() {
        return
    }
    set namespace = "auth-test/" _ ..Namespace
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("repo -delete -name " _ ..#RepoName))
    set sc = ##class(%IPM.Main).Shell("repo -o -name authsigned -url " _ ..#OrasURL _ " -namespace " _ namespace _ " -username writer -password writerpass -trust-server " _ ..#TrustServer _ " -publish 1")
    do $$$AssertStatusOK(sc, "Configured repo with credentials and a trust server")
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("load " _ ..GetModuleDir("oras-auth")))
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("publish oras-auth -r authsigned -only"), "Published with credentials")
    set ..Published("authsigned", "oras-auth") = ""
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("uninstall oras-auth"))

    set sc = ##class(%IPM.Main).Shell("install authsigned/oras-auth")
    do ..AssertRefused(sc, ..GUN("oras-auth", namespace))
    do $$$AssertNotTrue($system.Status.GetErrorText(sc) [ "writerpass", "Refusal does not show the password")

    do ..Sign("oras-auth", "1.0.0", , , , , namespace, "writer", "writerpass")
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("install authsigned/oras-auth"), "Installed with credentials")
    do $$$AssertEquals(##class(OrasAuth.Hello).Hello(), "hello")
}

// ===== Refusals =====

/// An unsigned module is refused before anything of it is unpacked, loaded or run, and history records the failure.
Method TestUnsignedModuleRefused()
{
    if '..NotaryRuns() {
        return
    }
    do ..PublishAppAndDep()
    do ..Sign(..#DepName, ..#DepTag)

    set sc = ##class(%IPM.Main).Shell("install " _ ..#RepoName _ "/" _ ..#AppName)
    do ..AssertRefused(sc, "does not have trust data for " _ ..GUN(..#AppName))
    do ..AssertLastHistoryFailed("install")
    do ..AssertNotInstalled(..#AppName)
    do ..AssertNotInstalled(..#DepName)
    do ..AssertNotExtracted(..#AppName, "1.0.0")
    do $$$AssertNotTrue($data(^OrasTrustApp), "Module lifecycle did not run")

    // A collection that signs other tags refuses this one too
    do ..Sign(..#AppName, "0.0.1", , , $translate($justify("", 64), " ", "0"), 1)
    set sc = ##class(%IPM.Main).Shell("install " _ ..#RepoName _ "/" _ ..#AppName)
    do ..AssertRefused(sc, "has no signed trust data for '" _ ..GUN(..#AppName) _ ":1.0.0'")
    do ..AssertNotExtracted(..#AppName, "1.0.0")
}

/// A signed module whose dependency is unsigned does not install, whether installed or loaded from a directory.
Method TestUnsignedDependencyRefused()
{
    if '..NotaryRuns() {
        return
    }
    do ..PublishAppAndDep()
    do ..Sign(..#AppName, "1.0.0")

    set sc = ##class(%IPM.Main).Shell("install " _ ..#RepoName _ "/" _ ..#AppName)
    do ..AssertRefused(sc, ..GUN(..#DepName))
    do ..AssertNotInstalled(..#DepName)
    do ..AssertNotInstalled(..#AppName)
    do ..AssertNotExtracted(..#DepName, ..#DepVersion)

    set appDirectory = ..GetModuleDir("oras-content-trust", "app-1.0.0")
    do ..AssertRefused(##class(%IPM.Main).Shell("load " _ appDirectory), ..GUN(..#DepName))
    do ..AssertNotInstalled(..#DepName)
    do ..AssertNotExtracted(..#DepName, ..#DepVersion)

    do ..Sign(..#DepName, ..#DepTag)
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("load " _ appDirectory), "Loaded with the signed dependency")
    do ..AssertVersion(..#DepName, ..#DepVersion)
}

/// Moving a signed tag to other content does not change what installs: the signed manifest is retrieved by its digest.
Method TestRetaggedContentNotConsumed()
{
    if '..NotaryRuns() {
        return
    }
    do ..PublishAppAndDep()
    do ..Sign(..#AppName, "1.0.0")
    do ..Sign(..#DepName, ..#DepTag)
    do ..LoadAndPublish("app-tampered", ..#AppName)
    do ..Uninstall(..#AppName, ..#DepName)
    kill ^OrasTrustApp

    do $$$AssertStatusOK(##class(%IPM.Main).Shell("install " _ ..#RepoName _ "/" _ ..#AppName), "Installed the signed content")
    do $$$AssertEquals(##class(OrasTrustApp.Info).Content(), "1.0.0", "Signed content installed, not what the tag points at")
    do $$$AssertNotTrue($data(^OrasTrustApp("configured", "tampered")), "Retagged content's lifecycle did not run")

    // Without the trust server, the repo serves whatever the tag points at
    do ..Uninstall(..#AppName)
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("repo -n " _ ..#RepoName _ " -trust-server """""))
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("install " _ ..#RepoName _ "/" _ ..#AppName))
    do $$$AssertEquals(##class(OrasTrustApp.Info).Content(), "tampered")
}

/// A package's unsigned listing data cannot make it stand in for another module. Its signed manifest has to give that name.
Method TestUnsignedNameClaimIgnored()
{
    if '..NotaryRuns() {
        return
    }
    set impostor = "oras-trust-other"
    do ..LoadAndPublish("other", impostor)
    do ..Uninstall(impostor)
    do ..Sign(impostor, "1.0.0")
    // The listing reads the module.xml annotation of whatever manifest the tag points at
    set manifest = {}.%FromJSON(..GetManifest(impostor, "1.0.0"))
    set key = "com.intersystems.ipm.module.v1+xml"
    set moduleXML = manifest.annotations.%Get(key)
    do $$$AssertTrue(moduleXML [ ("<Name>" _ impostor _ "</Name>"))
    do manifest.annotations.%Set(key, $replace(moduleXML, "<Name>" _ impostor _ "</Name>", "<Name>" _ ..#DepName _ "</Name>"))
    do ..PutManifest(impostor, "1.0.0", manifest.%ToJSON())

    // The app's dependency is never published
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("load " _ ..GetModuleDir("oras-content-trust", "dep")))
    do ..LoadAndPublish("app-1.0.0", ..#AppName)
    do ..Uninstall(..#AppName, ..#DepName)
    do ..Sign(..#AppName, "1.0.0")

    set sc = ##class(%IPM.Main).Shell("install " _ ..#RepoName _ "/" _ ..#AppName)
    do ..AssertRefused(sc, "Could not find satisfactory version of " _ ..#DepName)
    do ..AssertNotInstalled(impostor)
    do ..AssertNotInstalled(..#DepName)
    do ..AssertNotInstalled(..#AppName)
}

/// Trust data that does not chain from the pinned root is refused, as when a collection is replaced by someone without its keys.
Method TestReplacedRootRefused()
{
    if '..NotaryRuns() {
        return
    }
    do ..PublishAppAndDep()
    do ..Sign(..#AppName, "1.0.0")
    do ..Sign(..#DepName, ..#DepTag)
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("install " _ ..#RepoName _ "/" _ ..#AppName), "First install pins the root")
    do ..LoadAndPublish("app-tampered", ..#AppName)
    do ..Uninstall(..#AppName, ..#DepName)
    kill ^OrasTrustApp

    set gun = ..GUN(..#AppName)
    do ..RunNotary(..Collections(gun), .output, "delete", gun, "--remote")
    kill ..Collections(gun)
    set otherSigner = ##class(%IPM.Utils.File).CreateTempDirectory()
    do ..Sign(..#AppName, "1.0.0", otherSigner)

    set sc = ##class(%IPM.Main).Shell("install " _ ..#RepoName _ "/" _ ..#AppName)
    do ..AssertRefused(sc, "Could not verify trust data for '" _ gun _ "'")
    do ..AssertNotInstalled(..#AppName)
    do $$$AssertNotTrue($data(^OrasTrustApp), "Module lifecycle did not run")
    do ##class(%IPM.Utils.File).RemoveDirectoryTree(otherSigner)
}

/// Registry content that does not match its signed digest or size is refused before it is unpacked.
Method TestMismatchedContentRefused()
{
    if '..NotaryRuns() {
        return
    }
    do ..PublishAppAndDep()
    do ..Sign(..#DepName, ..#DepTag)
    set gun = ..GUN(..#AppName)
    set $listbuild(digest, size) = ..ManifestDigest(..#AppName, "1.0.0")

    do ..Sign(..#AppName, "1.0.0", , , digest, size + 1)
    set sc = ##class(%IPM.Main).Shell("install " _ ..#RepoName _ "/" _ ..#AppName)
    do ..AssertRefused(sc, "The manifest signed for '" _ gun _ ":1.0.0' from the registry is " _ size _ " bytes, but its signed size is " _ (size + 1) _ " bytes.")
    do ..AssertNotExtracted(..#AppName, "1.0.0")

    set missing = $translate($justify("", 64), " ", "e")
    do ..Sign(..#AppName, "1.0.0", , , missing, size)
    set sc = ##class(%IPM.Main).Shell("install " _ ..#RepoName _ "/" _ ..#AppName)
    do ..AssertRefused(sc, "has no manifest sha256:" _ missing _ ", which trust server " _ ..#TrustServer _ " signs for '" _ gun _ ":1.0.0'")
    do ..AssertNotExtracted(..#AppName, "1.0.0")

    // A signed manifest whose archive layer gives the wrong size
    set manifest = {}.%FromJSON(..GetManifest(..#AppName, "1.0.0"))
    set layer = manifest.layers.%Get(0)
    set archiveSize = layer.size
    set layer.size = archiveSize + 1
    do ..PutManifest(..#AppName, "1.0.0", manifest.%ToJSON())
    do ..Sign(..#AppName, "1.0.0")
    set sc = ##class(%IPM.Main).Shell("install " _ ..#RepoName _ "/" _ ..#AppName)
    do ..AssertRefused(sc, "The module archive signed for '" _ gun _ ":1.0.0' from the registry is " _ archiveSize _ " bytes, but its signed size is " _ (archiveSize + 1) _ " bytes.")
    do ..AssertNotInstalled(..#AppName)
    do ..AssertNotExtracted(..#AppName, "1.0.0")
}

/// An unreachable trust server fails the install, even with trust data pinned from an earlier verification.
Method TestTrustServerUnavailable()
{
    if '..NotaryRuns() {
        return
    }
    do ..PublishAppAndDep()
    do ..Sign(..#AppName, "1.0.0")
    do ..Sign(..#DepName, ..#DepTag)
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("install " _ ..#RepoName _ "/" _ ..#AppName), "First install pins trust data")
    do $$$AssertTrue(##class(%IPM.Repo.Oras.TrustedCollection).GUNIndexExists(..GUN(..#AppName)), "Trust data is pinned")
    do ..Uninstall(..#AppName, ..#DepName)
    do ..RemoveExtracted(..#AppName)

    set unreachable = "https://notary:1"
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("repo -n " _ ..#RepoName _ " -trust-server " _ unreachable))
    set sc = ##class(%IPM.Main).Shell("install " _ ..#RepoName _ "/" _ ..#AppName)
    do ..AssertRefused(sc, "Could not verify trust data for '" _ ..GUN(..#AppName) _ "' from trust server " _ unreachable)
    do ..AssertNotInstalled(..#AppName)
    do ..AssertNotExtracted(..#AppName, "1.0.0")
}

/// A notary client that cannot be run fails the install rather than skipping verification.
Method TestVerifierUnavailable()
{
    if '..NotaryRuns() {
        return
    }
    do ..PublishAppAndDep()
    do ..Sign(..#AppName, "1.0.0")
    do ..Sign(..#DepName, ..#DepTag)

    do $$$AssertStatusOK(##class(%IPM.Main).Shell("config set NotaryPath /nonexistent/notary"))
    set sc = ##class(%IPM.Main).Shell("install " _ ..#RepoName _ "/" _ ..#AppName)
    do ..AssertRefused(sc, "Could not run the notary client '/nonexistent/notary'")
    do ..AssertNotInstalled(..#AppName)
    do ..AssertNotExtracted(..#AppName, "1.0.0")

    do $$$AssertStatusOK(##class(%IPM.Main).Shell("config delete NotaryPath"))
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("install " _ ..#RepoName _ "/" _ ..#AppName), "Installed with the notary client on the PATH")
}

/// update, reinstall, and install with -force or -dev all verify, and a refusal leaves the installed version working.
Method TestUpdateAndReinstallVerify()
{
    if '..NotaryRuns() {
        return
    }
    do ..LoadAndPublish("dep", ..#DepName)
    do ..LoadAndPublish("app-1.0.0", ..#AppName)
    do ..LoadAndPublish("app-2.0.0", ..#AppName)
    do ..Uninstall(..#AppName, ..#DepName)
    do ..Sign(..#AppName, "1.0.0")
    do ..Sign(..#DepName, ..#DepTag)
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("install " _ ..#RepoName _ "/" _ ..#AppName _ " 1.0.0"), "Installed 1.0.0")
    kill ^OrasTrustApp

    set sc = ##class(%IPM.Main).Shell("update " _ ..#AppName)
    do ..AssertRefused(sc, "has no signed trust data for '" _ ..GUN(..#AppName) _ ":2.0.0'")
    do ..AssertLastHistoryFailed("update")
    do ..AssertVersion(..#AppName, "1.0.0")
    do $$$AssertEquals(##class(OrasTrustApp.Info).Describe(), "app 1.0.0 with dep " _ ..#DepVersion, "Installed version still works")
    do ..AssertNotExtracted(..#AppName, "2.0.0")
    do $$$AssertNotTrue($data(^OrasTrustApp), "Refused version's lifecycle did not run")

    set sc = ##class(%IPM.Main).Shell("install " _ ..#RepoName _ "/" _ ..#AppName _ " 2.0.0 -force -dev")
    do ..AssertRefused(sc, "has no signed trust data for '" _ ..GUN(..#AppName) _ ":2.0.0'")
    do ..AssertVersion(..#AppName, "1.0.0")

    do ..Sign(..#AppName, "2.0.0")
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("update " _ ..#AppName), "Updated to signed 2.0.0")
    do ..AssertVersion(..#AppName, "2.0.0")
    do $$$AssertEquals(##class(OrasTrustApp.Info).Content(), "2.0.0")

    do $$$AssertStatusOK(##class(%IPM.Main).Shell("config set NotaryPath /nonexistent/notary"))
    do ..AssertRefused(##class(%IPM.Main).Shell("reinstall " _ ..#AppName), "Could not run the notary client")
    do ..AssertVersion(..#AppName, "2.0.0")
    do $$$AssertEquals(##class(OrasTrustApp.Info).Content(), "2.0.0", "Installed version still works")
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("config delete NotaryPath"))
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("reinstall " _ ..#AppName), "Reinstalled signed 2.0.0")
    do ..AssertVersion(..#AppName, "2.0.0")
}

// ===== Helpers =====

/// Returns whether the notary client can run here, failing the test if not.
Method NotaryRuns() As %Boolean
{
    return $$$AssertNotTrue(..NotaryMissing, "The notary client runs. Put it on the PATH to run the content trust tests.")
}

ClassMethod CanRunNotary() As %Boolean
{
    set outFile = ##class(%File).TempFilename("txt")
    try {
        set exitCode = $zf(-100, "/STDOUT=""" _ outFile _ """ /STDERR=""" _ outFile _ """", "notary", "version")
    } catch {
        set exitCode = -1
    }
    do ##class(%File).Delete(outFile)
    return (exitCode = 0)
}

/// Returns the collection name of package in the current test's namespace or in namespace, e.g. "oras:5000/content-trust/.../oras-trust-app".
Method GUN(
	package As %String,
	namespace As %String = {..Namespace}) As %String
{
    return ..#RegistryHost _ "/" _ namespace _ "/" _ package
}

/// Signs tag of package as an external promotion process would, with the manifest's digest and size from the registry unless given.
/// Creates the collection on the trust server first if the test has not.
/// With role, the target is signed by that delegation instead of the targets role.
Method Sign(
	package As %String,
	tag As %String,
	signer As %String = {..SignerDirectory},
	role As %String = "",
	digest As %String = "",
	size As %String = "",
	namespace As %String = {..Namespace},
	username As %String = "",
	password As %String = "")
{
    if (digest = "") {
        set $listbuild(digest, size) = ..ManifestDigest(package, tag, namespace, username, password)
    }
    set gun = ..GUN(package, namespace)
    if '$data(..Collections(gun)) {
        do $$$AssertTrue(..RunNotary(signer, .output, "init", gun, "-p"), "Created collection " _ gun _ ": " _ output)
        set ..Collections(gun) = signer
    }
    if (role = "") {
        set ok = ..RunNotary(signer, .output, "addhash", gun, tag, size, "--sha256", digest, "-p")
    } else {
        set ok = ..RunNotary(signer, .output, "addhash", gun, tag, size, "--sha256", digest, "--roles", role, "-p")
    }
    do $$$AssertTrue(ok, "Signed " _ gun _ ":" _ tag _ $select(role = "": "", 1: " as " _ role) _ ": " _ output)
}

/// Adds delegation role to the collection of package with a new key, which the signer can then sign with.
Method AddDelegation(
	package As %String,
	role As %String)
{
    set gun = ..GUN(package)
    if '$data(..Collections(gun)) {
        do $$$AssertTrue(..RunNotary(..SignerDirectory, .output, "init", gun, "-p"), "Created collection " _ gun _ ": " _ output)
        set ..Collections(gun) = ..SignerDirectory
    }
    set base = ##class(%File).TempFilename()
    set exitCode = $zf(-100, "/STDOUT=""" _ base _ ".log"" /STDERR=""" _ base _ ".log""", "openssl", "req", "-new", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:prime256v1", "-nodes", "-x509", "-days", "3650", "-subj", "/CN=" _ $piece(role, "/", *), "-keyout", base _ ".key", "-out", base _ ".crt")
    do $$$AssertEquals(exitCode, 0, "Generated a key and certificate for " _ role)
    do $$$AssertTrue(..RunNotary(..SignerDirectory, .output, "delegation", "add", gun, role, base _ ".crt", "--all-paths", "-p"), "Added delegation " _ role _ ": " _ output)
    do $$$AssertTrue(..RunNotary(..SignerDirectory, .output, "key", "import", base _ ".key", "--role", role), "Imported key for " _ role _ ": " _ output)
    for extension = ".key", ".crt", ".log" {
        do ##class(%File).Delete(base _ extension)
    }
}

/// Runs the notary client against the trust server with trustDirectory, returning whether it succeeded and its output in output.
ClassMethod RunNotary(
	trustDirectory As %String,
	Output output As %String,
	args... As %String) As %Boolean
{
    set outFile = ##class(%File).TempFilename("txt")
    set errFile = ##class(%File).TempFilename("txt")
    set cmd = 0
    // Passphrases for the keys the client creates. They protect nothing outside the test.
    for arg = "NOTARY_ROOT_PASSPHRASE=root-pass", "NOTARY_TARGETS_PASSPHRASE=targets-pass", "NOTARY_SNAPSHOT_PASSPHRASE=snapshot-pass", "NOTARY_DELEGATION_PASSPHRASE=delegation-pass", "notary", "-s", ..#TrustServer, "-d", trustDirectory {
        set cmd($increment(cmd)) = arg
    }
    for i = 1:1:$get(args) {
        set cmd($increment(cmd)) = args(i)
    }
    set exitCode = $zf(-100, "/STDOUT=""" _ outFile _ """ /STDERR=""" _ errFile _ """", "env", cmd...)
    set output = ..ReadFile(outFile) _ ..ReadFile(errFile)
    do ##class(%File).Delete(outFile)
    do ##class(%File).Delete(errFile)
    return (exitCode = 0)
}

ClassMethod ReadFile(filename As %String) As %String
{
    set file = ##class(%Stream.FileCharacter).%New()
    do file.LinkToFile(filename)
    set content = ""
    while 'file.AtEnd {
        set content = content _ file.Read()
    }
    return content
}

/// Returns $listbuild(sha256 hex digest, size) of the manifest the registry has for package:tag.
Method ManifestDigest(
	package As %String,
	tag As %String,
	namespace As %String = {..Namespace},
	username As %String = "",
	password As %String = "") As %List
{
    set result = ..ManifestDigestPython(..#OrasURL _ "/v2/" _ namespace _ "/" _ package _ "/manifests/" _ tag, username, password)
    return $listfromstring(result, " ")
}

ClassMethod ManifestDigestPython(
	url As %String,
	username As %String,
	password As %String) As %String [ Language = python ]
{
    import hashlib
    import requests

    auth = (username, password) if username else None
    response = requests.get(url, headers={"Accept": "application/vnd.oci.image.manifest.v1+json"}, auth=auth, timeout=30)
    response.raise_for_status()
    return f"{hashlib.sha256(response.content).hexdigest()} {len(response.content)}"
}

/// Returns the text of the manifest the registry has for package:tag in the current test's namespace.
Method GetManifest(
	package As %String,
	tag As %String) As %String
{
    return ..HttpManifest("GET", ..#OrasURL _ "/v2/" _ ..Namespace _ "/" _ package _ "/manifests/" _ tag, "")
}

/// Points tag of package at the manifest with the given text, as a registry that someone has tampered with would.
Method PutManifest(
	package As %String,
	tag As %String,
	text As %String)
{
    do ..HttpManifest("PUT", ..#OrasURL _ "/v2/" _ ..Namespace _ "/" _ package _ "/manifests/" _ tag, text)
}

/// Points tag of package at a manifest whose module.xml annotation is a whole module.xml file with line breaks around it,
/// as other publishing tools can write it. The XML declaration is only valid once the line break before it is trimmed.
Method PadModuleXML(
	package As %String,
	tag As %String)
{
    set manifest = {}.%FromJSON(..GetManifest(package, tag))
    set key = "com.intersystems.ipm.module.v1+xml"
    set moduleXML = $char(10) _ "<?xml version=""1.0"" encoding=""UTF-8""?>" _ $char(10) _ manifest.annotations.%Get(key) _ $char(13, 10)
    do manifest.annotations.%Set(key, moduleXML)
    do ..PutManifest(package, tag, manifest.%ToJSON())
}

ClassMethod HttpManifest(
	method As %String,
	url As %String,
	text As %String) As %String [ Language = python ]
{
    import requests

    media_type = "application/vnd.oci.image.manifest.v1+json"
    if method == "PUT":
        response = requests.put(url, data=text.encode("utf-8"), headers={"Content-Type": media_type}, timeout=30)
    else:
        response = requests.get(url, headers={"Accept": media_type}, timeout=30)
    response.raise_for_status()
    return response.text
}

/// Loads the fixture in _data/oras-content-trust/fixture and publishes module from it to repo.
Method LoadAndPublish(
	fixture As %String,
	module As %String,
	repo As %String = {..#RepoName})
{
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("load " _ ..GetModuleDir("oras-content-trust", fixture)), "Loaded " _ fixture)
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("publish " _ module _ " -r " _ repo _ " -only"), "Published " _ module _ " from " _ fixture)
    set ..Published(repo, module) = ""
}

/// Publishes the dependency and app 1.0.0 without signing them, then uninstalls them.
Method PublishAppAndDep()
{
    do ..LoadAndPublish("dep", ..#DepName)
    do ..LoadAndPublish("app-1.0.0", ..#AppName)
    do ..Uninstall(..#AppName, ..#DepName)
    kill ^OrasTrustApp
}

/// Loads directory with developer mode off and packages module, as publishing a deployed module needs.
Method LoadAndPackage(
	directory As %String,
	module As %String)
{
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("load -nodev " _ directory), "Loaded " _ directory)
    set path = $$$FileTempDir _ "content-trust-" _ module
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("package " _ module _ " -only -path " _ path), "Packaged " _ module)
    do ##class(%File).Delete(path _ ".tgz")
}

/// Packages the loaded module and publishes it to the repo as deployed for platform, which the publish command can only do for the current platform.
/// The registry package is named package.
Method PublishForPlatform(
	module As %String,
	platform As %String,
	package As %String = {module})
{
    set installed = ##class(%IPM.Storage.Module).NameOpen(module, , .sc)
    do $$$AssertStatusOK(sc)
    set moduleInfo = ##class(%IPM.Repo.Remote.ModuleInfo).%New()
    set moduleInfo.Name = package
    set moduleInfo.VersionString = installed.VersionString
    set moduleInfo.Deployed = 1
    set moduleInfo.PlatformVersion = platform
    set path = $$$FileTempDir _ "content-trust-" _ module
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("package " _ module _ " -only -path " _ path), "Packaged " _ module)
    set payload = ##class(%Stream.FileBinary).%New()
    do payload.LinkToFile(path _ ".tgz")
    do moduleInfo.Payload.CopyFrom(payload)
    set manifest = ##class(%Stream.GlobalCharacter).%New()
    do $$$AssertStatusOK(installed.XMLExportToStream(manifest))
    set moduleInfo.Manifest = manifest
    set service = ##class(%IPM.Repo.Oras.Definition).ServerDefinitionKeyOpen(..#RepoName).GetPublishService()
    do $$$AssertStatusOK(service.PublishModule(moduleInfo), "Published " _ module _ " for platform " _ platform)
    do ##class(%File).Delete(path _ ".tgz")
}

Method Uninstall(modules... As %String)
{
    for i = 1:1:$get(modules) {
        do $$$AssertStatusOK(##class(%IPM.Main).Shell("uninstall " _ modules(i)), "Uninstalled " _ modules(i))
    }
}

/// Directory where an install from a repo unpacks module.
ClassMethod ExtractionDirectory(
	module As %String,
	version As %String = "") As %String
{
    return ##class(%File).NormalizeDirectory(##class(%IPM.Repo.UniversalSettings).GetModuleRoot() _ module _ "/" _ version)
}

ClassMethod RemoveExtracted(module As %String)
{
    set directory = ..ExtractionDirectory(module)
    if ##class(%File).DirectoryExists(directory) {
        do ##class(%IPM.Utils.File).RemoveDirectoryTree(directory)
    }
}

/// Asserts that sc is an error whose text contains each of the substrings.
Method AssertRefused(
	sc As %Status,
	substrings... As %String)
{
    do $$$AssertStatusNotOK(sc, "Refused")
    set text = $system.Status.GetErrorText(sc)
    for i = 1:1:$get(substrings) {
        do $$$AssertTrue(text [ substrings(i), "Error contains: " _ substrings(i) _ $char(13,10) _ "Error was: " _ text)
    }
}

Method AssertNotInstalled(module As %String)
{
    do $$$AssertNotTrue(##class(%IPM.Storage.Module).NameExists(module), module _ " is not installed")
}

Method AssertNotExtracted(
	module As %String,
	version As %String)
{
    do $$$AssertNotTrue(##class(%File).DirectoryExists(..ExtractionDirectory(module, version)), module _ " " _ version _ " was not unpacked")
}

/// Asserts that module is installed at version.
Method AssertVersion(
	module As %String,
	version As %String)
{
    set installed = ##class(%IPM.Storage.Module).NameOpen(module)
    if $$$AssertTrue($isobject(installed), module _ " is installed") {
        do $$$AssertEquals(installed.VersionString, version)
    }
}

/// Asserts that the latest history entry is a failed action.
Method AssertLastHistoryFailed(action As %String)
{
    set rs = ##class(%IPM.General.History).GetHistory(, , 1)
    if $$$AssertTrue(rs.%Next(), "History has an entry") {
        do $$$AssertEquals(rs.%Get("Action"), action)
        do $$$AssertNotEquals(rs.%Get("Success"), 1, "History records the " _ action _ " as failed")
    }
}

}
