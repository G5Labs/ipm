Include %IPM.Common

/// Integration tests for ORAS repos configured with a Docker Content Trust server (repo -trust-server).
/// Modules are published with the ordinary publish command and signed afterwards with the notary CLI,
/// as an external promotion process would. Needs the oras and notary services of docker-compose.yml
/// and the notary CLI in the IRIS container.
/// <p>
/// Each test publishes to its own registry namespace below one chosen per run. The notary service keeps
/// trust data for as long as it runs and IPM pins the root key of each repository it has verified,
/// so reusing a repository name across runs would meet the signing keys of an earlier run.
Class Test.PM.Integration.OrasContentTrust Extends Test.PM.Integration.Base
{

Parameter OrasURL = "http://oras:5000";

/// Registry host as it appears in OCI references and Docker Content Trust repository names.
Parameter Registry = "oras:5000";

Parameter TrustServerURL = "http://notary:4443";

/// A port on the notary host where nothing listens, standing in for a trust server that is down.
Parameter DownTrustServerURL = "http://notary:4444";

Parameter FixtureRoot = "oras-content-trust";

/// Registry namespace this run publishes under.
Property RunNamespace As %String;

/// Registry namespace of the current test, below RunNamespace. Not named Test..., since %UnitTest
/// would run the property's generated methods as tests.
Property CaseNamespace As %String;

/// Key directory of the external signer.
Property SignerDirectory As %String;

/// Why the tests cannot run here, or "" when they can.
Property Unavailable As %String;

Property SavedNotaryExecutable As %String;

Method OnBeforeAllTests() As %Status
{
    set ..RunNamespace = "content-trust-" _ $translate($zdatetime($horolog, 8), ": ") _ "-" _ $job
    set ..SignerDirectory = ##class(%IPM.Utils.File).CreateTempDirectory()
    set ..SavedNotaryExecutable = ##class(%IPM.Repo.UniversalSettings).GetValue(##class(%IPM.Repo.UniversalSettings).#NotaryExecutable)
    set out = ##class(%Stream.TmpCharacter).%New()
    set sc = ##class(%IPM.Utils.Module).RunCommand(..SignerDirectory, $listbuild(##class(%IPM.Repo.Oras.ContentTrust).NotaryExecutable(), "version"), .out, .out)
    if $$$ISERR(sc) {
        set ..Unavailable = "The notary CLI is not available: " _ $system.Status.GetErrorText(sc)
    } elseif '..TrustServerHealthy() {
        set ..Unavailable = "Trust server " _ ..#TrustServerURL _ " is not reachable"
    }
    return $$$OK
}

Method OnBeforeOneTest(testName As %String) As %Status
{
    set ..CaseNamespace = ..RunNamespace _ "/" _ $zconvert(testName, "L")
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("repo -delete-all"), "Deleted repos")
    do ..ConfigureTrustedRepo(..CaseNamespace)
    return $$$OK
}

/// Removes what the test installed and published. Other ORAS tests search the whole registry,
/// so a package left in this test's namespace would show up in them.
Method OnAfterOneTest(testName As %String) As %Status
{
    do ##class(%IPM.Repo.UniversalSettings).SetValue(##class(%IPM.Repo.UniversalSettings).#NotaryExecutable, ..SavedNotaryExecutable)
    for module = "oras-dep-consumer-name", "default-name", "trust-parent", "trust-child", "trust-app", "oras-version-with-build", "platform-filter-test", "oras-auth" {
        do ##class(%IPM.Main).Shell("uninstall " _ module)
    }
    if ##class(%IPM.Repo.Definition).ServerDefinitionKeyExists("trusted") {
        for module = "trust-parent", "trust-child", "trust-app", "oras-version-with-build", "platform-filter-test", "oras-auth", "external-name" {
            if ..PublishedTags(..CaseNamespace _ "/" _ module) '= "" {
                do $$$AssertStatusOK(##class(%IPM.Main).Shell("unpublish trusted/" _ module _ " all -force"), "Unpublished " _ module)
            }
        }
    }
    return $$$OK
}

Method OnAfterAllTests() As %Status
{
    do ##class(%IPM.Main).Shell("repo -delete-all")
    do ##class(%IPM.Utils.File).RemoveDirectoryTree(..SignerDirectory)
    return $$$OK
}

// ===== Configuration =====

/// The trust server is stored, read back, passed to the package service, kept by the lock file and by
/// repo -copy-from, and can be cleared. Records saved before the setting existed read as unprotected.
Method TestRepoConfiguration()
{
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("repo -o -name trusted -trust-server HTTP://Notary:4443/"), "Set the trust server in another form")
    set repo = ##class(%IPM.Repo.Oras.Definition).ServerDefinitionKeyOpen("trusted")
    do $$$AssertEquals(repo.TrustServer, ..#TrustServerURL, "The trust server is stored canonically")
    do $$$AssertEquals(repo.GetPackageService().TrustServer, ..#TrustServerURL, "The package service uses the trust server")
    do $$$AssertEquals(repo.%ConstructClone(1).TrustServer, ..#TrustServerURL, "repo -copy-from's deep clone keeps the trust server")
    do $$$AssertStatusOK(repo.%JSONExportToString(.lockEntry, "LockFileMapping"))
    do $$$AssertTrue(lockEntry [ """trustServer"":""http://notary:4443""", "Lock file entry has the trust server: " _ lockEntry)

    set cookie = ""
    do ##class(%IPM.Utils.Module).BeginCaptureOutput(.cookie)
    do ##class(%IPM.Main).Shell("repo -list")
    do ##class(%IPM.Utils.Module).EndCaptureOutput(cookie, .lines)
    do $$$AssertTrue(..FindStringInMultiDimArray("Trust server:", .lines), "repo -list shows the trust server")

    set sc = ##class(%IPM.Main).Shell("repo -o -name trusted -trust-server ftp://notary:4443")
    do $$$AssertStatusNotOK(sc, "A trust server that is not http or https is refused")
    do $$$AssertEquals(##class(%IPM.Repo.Oras.Definition).ServerDefinitionKeyOpen("trusted").TrustServer, ..#TrustServerURL, "The refused value was not saved")

    do $$$AssertStatusOK(##class(%IPM.Main).Shell("repo -o -name trusted -trust-server """""), "Cleared the trust server")
    do $$$AssertEquals(##class(%IPM.Repo.Oras.Definition).ServerDefinitionKeyOpen("trusted").TrustServer, "", "The trust server is cleared")

    // Other repository types do not check signatures, so they refuse the setting rather than ignore it
    set sc = ##class(%IPM.Main).Shell("repo -fs -name untrusted-fs -path " _ ..FixtureDir("trust-app-1.0.0") _ " -trust-server " _ ..#TrustServerURL)
    do $$$AssertStatusNotOK(sc, "A filesystem repo refuses -trust-server")
    do $$$AssertNotTrue(##class(%IPM.Repo.Definition).ServerDefinitionKeyExists("untrusted-fs"), "The refused repo was not created")

    // config delete restores finding notary on the PATH, not the first path ever set
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("config set NotaryExecutable /nonexistent/notary"))
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("config delete NotaryExecutable"))
    do $$$AssertEquals(##class(%IPM.Repo.Oras.ContentTrust).NotaryExecutable(), "notary", "config delete resets the verifier path")

    // A definition saved by an earlier version has no slot for the trust server
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("repo -o -name trusted -trust-server " _ ..#TrustServerURL))
    kill repo
    set id = ##class(%IPM.Repo.Oras.Definition).ServerDefinitionKeyOpen("trusted").%Id()
    set node = $name(^IPM.Repo.DefinitionD(id, "OrasRepoDefinition"))
    do $$$AssertEquals($listget(@node, 7), ..#TrustServerURL, "The trust server is stored in the seventh slot")
    set @node = $list(@node, 1, 6)
    set old = ##class(%IPM.Repo.Oras.Definition).%OpenId(id, 0)
    do $$$AssertStatusOK(old.%Reload(), "Reread the definition from storage")
    do $$$AssertEquals(old.TrustServer, "", "An earlier record reads with no trust server")
    do $$$AssertEquals(old.GetPackageService().TrustServer, "", "An earlier record gives an unprotected package service")
}

// ===== Signed and unsigned packages =====

/// A package published with the ordinary publish command is refused until it is signed outside IPM, then installs.
Method TestSignedInstall()
{
    if '..Available() {
        return
    }
    do ..Publish("trust-app-1.0.0", "trust-app")
    do ..Publish("trust-app-2.0.0", "trust-app")

    set sc = ..Run("install trusted/trust-app 1.0.0", .lines)
    do ..AssertRefused(sc, .lines, "no signature for them: 1.0.0")
    do ..AssertNotInstalled("trust-app")
    set reference = ##class(%IPM.Storage.ModuleInfo).%New()
    set reference.Name = "trust-app"
    set reference.VersionString = "1.0.0"
    do $$$AssertNotTrue(##class(%IPM.Repo.Oras.Definition).ServerDefinitionKeyOpen("trusted").GetPackageService().HasModule(reference), "HasModule ignores the unsigned version")

    // The same manifest signed for another repository does not count for this one
    set manifest = {}.%FromJSON(..ReadManifest(..#Registry, ..CaseNamespace _ "/trust-app", "1.0.0"))
    set otherGUN = ..GUN("trust-app-mirror")
    do ..InitTrust(otherGUN)
    do ..Notary(..SignerDirectory, ["addhash", (otherGUN), "1.0.0", (manifest.size), "--sha256", ($piece(manifest.digest, ":", 2)), "-p"])
    set sc = ..Run("install trusted/trust-app 1.0.0", .lines)
    do ..AssertRefused(sc, .lines, "no signature for them: 1.0.0")
    do ..AssertNotInstalled("trust-app")

    do ..Sign("trust-app", "1.0.0")
    do $$$AssertTrue(##class(%IPM.Repo.Oras.Definition).ServerDefinitionKeyOpen("trusted").GetPackageService().HasModule(reference), "HasModule finds the signed version")
    do $$$AssertStatusOK(..Run("install trusted/trust-app", .lines), "Installed the newest signed version")
    do ..AssertRevision("1.0.0", "1.0.0 revision 1")
    do ..AssertLatestHistory("install", "trust-app", 1)

    // Without the setting, the same repo installs unsigned packages again
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("repo -o -name trusted -trust-server """""), "Turned verification off")
    do $$$AssertStatusOK(..Run("install trusted/trust-app 2.0.0", .lines), "Installed the unsigned package")
    do ..AssertRevision("2.0.0", "2.0.0")
}

/// As docker pull does, a signature in targets/releases takes priority over one in targets,
/// and a signature only in another delegation is not trusted.
Method TestTrustedRoles()
{
    if '..Available() {
        return
    }
    do ..Publish("trust-app-1.0.0", "trust-app")
    do ..Sign("trust-app", "1.0.0")
    do ..Publish("trust-app-1.0.0-rev2", "trust-app")
    do ..AddDelegation("trust-app", "targets/releases")
    do ..Sign("trust-app", "1.0.0", "targets/releases")
    do ..Publish("trust-app-2.0.0", "trust-app")
    do ..AddDelegation("trust-app", "targets/other")
    do ..Sign("trust-app", "2.0.0", "targets/other")

    do $$$AssertStatusOK(..Run("install trusted/trust-app", .lines), "Installed the newest trusted version")
    do ..AssertRevision("1.0.0", "1.0.0 revision 2")
    set sc = ..Run("update trust-app 2.0.0", .lines)
    do ..AssertRefused(sc, .lines, "no signature for them: 2.0.0")
    do ..AssertRevision("1.0.0", "1.0.0 revision 2")
}

/// The signed manifest is what installs, even after the tag moves to another manifest.
/// When the registry no longer has the signed manifest, the install is refused.
Method TestSignedArtifactIsTheOneInstalled()
{
    if '..Available() {
        return
    }
    do ..Publish("trust-app-1.0.0", "trust-app")
    set signed = ..Sign("trust-app", "1.0.0")
    do ..Publish("trust-app-1.0.0-rev2", "trust-app")
    do $$$AssertNotEquals(..ManifestDigest("trust-app", "1.0.0"), signed, "The tag now names an unsigned manifest")

    do $$$AssertStatusOK(..Run("install trusted/trust-app 1.0.0", .lines), "Installed 1.0.0")
    do ..AssertRevision("1.0.0", "1.0.0 revision 1")

    // The repo stops offering a version whose signed manifest is gone, rather than installing what the tag names now
    do ..DeleteManifest("trust-app", signed)
    set sc = ..Run("install trusted/trust-app 1.0.0 -force", .lines)
    do ..AssertRefused(sc, .lines, "trust-app 1.0.0 not found")
    do ..AssertRevision("1.0.0", "1.0.0 revision 1")
    do ..AssertLatestHistory("install", "trust-app", 0)
}

/// Within one shell command the signed listing is read once. A tag signed again between dependency planning
/// and download does not change the package the command loads. The next command sees the new signature.
Method TestSnapshotWithinCommand()
{
    if '..Available() {
        return
    }
    do ..Publish("trust-app-1.0.0", "trust-app")
    set first = ..Sign("trust-app", "1.0.0")
    do ..Publish("trust-app-1.0.0-rev2", "trust-app")
    set reference = ##class(%IPM.Storage.ModuleInfo).%New()
    set reference.Name = "trust-app"
    set reference.VersionString = "1.0.0"

    // The lookup state a shell command starts with
    new $$$RepoLookupState
    set $$$RepoLookupState = ""
    set service = ##class(%IPM.Repo.Oras.Definition).ServerDefinitionKeyOpen("trusted").GetPackageService()
    do $$$AssertTrue($isobject(service.GetModuleManifest(reference)), "Read the manifest for dependency planning")
    set second = ..Sign("trust-app", "1.0.0")
    do $$$AssertNotEquals(second, first, "The tag is now signed for the second revision")
    do $$$AssertEquals(..StreamDigest(service.GetModule(reference)), ..PackageDigest("trust-app", first), "The command downloads the package it planned with")

    kill $$$RepoLookupState
    set $$$RepoLookupState = ""
    do $$$AssertEquals(..StreamDigest(service.GetModule(reference)), ..PackageDigest("trust-app", second), "The next command downloads the newly signed package")
}

/// Bytes that do not match what was signed are refused before anything is unpacked.
Method TestMismatchedContentRefused()
{
    if '..Available() {
        return
    }
    do ..Publish("trust-app-1.0.0", "trust-app")

    // The signature names the right digest with the wrong size
    do ..Sign("trust-app", "1.0.0", , , 1)
    set sc = ..Run("install trusted/trust-app 1.0.0", .lines)
    do ..AssertRefused(sc, .lines, "does not match the signed manifest")
    do ..AssertNotInstalled("trust-app")

    // A signed manifest whose package layer is one byte longer than the package
    do ..PushModifiedManifest("trust-app", "1.0.0", "1.0.1", "size")
    do ..Sign("trust-app", "1.0.1")
    set sc = ..Run("install trusted/trust-app 1.0.1", .lines)
    do ..AssertRefused(sc, .lines, "does not match its signed descriptor")
    do ..AssertNotInstalled("trust-app")
    do ..AssertLatestHistory("install", "trust-app", 0)

    // A signed manifest with two package layers does not identify one package
    do ..PushModifiedManifest("trust-app", "1.0.0", "1.0.2", "duplicate")
    do ..Sign("trust-app", "1.0.2")
    set sc = ..Run("install trusted/trust-app 1.0.2", .lines)
    do ..AssertRefused(sc, .lines, "more than one package layer")
    do ..AssertNotInstalled("trust-app")

    // A signed manifest whose package layer is a valid blob that is not an archive reaches extraction and fails there
    do ..PushModifiedManifest("trust-app", "1.0.0", "1.0.4", "config")
    do ..Sign("trust-app", "1.0.4")
    set sc = ..Run("install trusted/trust-app 1.0.4", .lines)
    do $$$AssertStatusNotOK(sc, "A signed package that is not an archive does not install")
    do ..AssertNotInstalled("trust-app")
    do ..AssertLatestHistory("install", "trust-app", 0)
}

/// The registry's bytes must have the signed digest, not just the signed size. zot cannot serve wrong bytes for a
/// digest, so a stand-in server does.
Method TestWrongBytesRefused()
{
    if '..Available() {
        return
    }
    set port = 18471
    do ..StartStub("serve", port, 60)
    set registry = "http://127.0.0.1:" _ port
    set repository = "127.0.0.1:" _ port _ "/tampered/module"
    // The stand-in serves 64 bytes of "x"; the signature is for 64 bytes of "y"
    set signed = "sha256:" _ $zconvert(##class(%xsd.hexBinary).LogicalToXSD($system.Encryption.SHAHash(256, $translate($justify("", 64), " ", "y"))), "L")
    try {
        do ##class(%IPM.Repo.Oras.PackageService).FetchTrustedManifest(registry, repository, signed, 64, "", "", "", "apiKey")
        do $$$AssertFailure("Accepted a manifest with the wrong digest")
    } catch e {
        set text = $system.Status.GetErrorText(##class(%IPM.Repo.Oras.PackageService).MismatchAsError(e).AsStatus())
        do $$$AssertTrue(text [ "does not match the signed manifest of 64 bytes", text)
    }
    set file = ##class(%IPM.Utils.File).CreateTempDirectory() _ "package.tgz"
    try {
        do ##class(%IPM.Repo.Oras.PackageService).DownloadTrustedBlob(registry, repository, signed, 64, file, "", "", "", "apiKey")
        do $$$AssertFailure("Accepted a package with the wrong digest")
    } catch e {
        set text = $system.Status.GetErrorText(##class(%IPM.Repo.Oras.PackageService).MismatchAsError(e).AsStatus())
        do $$$AssertTrue(text [ "does not match its signed descriptor of 64 bytes", text)
    }
    do $$$AssertNotTrue(##class(%File).Exists(file), "The refused download was deleted")
}

/// A trust server that answers notary's first request and then stalls fails the check instead of hanging it.
Method TestStalledTrustServerRefused()
{
    if '..Available() {
        return
    }
    if ($system.Version.GetOS() '= "UNIX") {
        do $$$AssertSkipped("The deadline uses the Unix timeout program")
        return
    }
    set port = 18472
    do ..StartStub("stall", port, 60)
    set start = $zhorolog
    try {
        do ##class(%IPM.Repo.Oras.ContentTrust).LookUpSignedTargets("http://127.0.0.1:" _ port, "oras:5000/stalled/module", 3)
        do $$$AssertFailure("A stalled trust server answered")
    } catch e {
        set text = $system.Status.GetErrorText(e.AsStatus())
        do $$$AssertTrue(text [ "did not finish within 3 seconds", text)
    }
    do $$$AssertTrue(($zhorolog - start) < 30, "Gave up after " _ ($zhorolog - start) _ "s")

    // Within one command, the next repository on the stalled server fails without waiting again
    new $$$RepoLookupState
    set $$$RepoLookupState = ""
    set server = "http://127.0.0.1:" _ port
    try {
        do ##class(%IPM.Repo.Oras.ContentTrust).SignedTargets(server, "oras:5000/stalled/first", "stalled-repo", 3)
        do $$$AssertFailure("A stalled trust server answered for the first repository")
    } catch e {
        do $$$AssertTrue($system.Status.GetErrorText(e.AsStatus()) [ "did not finish within 3 seconds")
    }
    set start = $zhorolog
    try {
        do ##class(%IPM.Repo.Oras.ContentTrust).SignedTargets(server, "oras:5000/stalled/second", "stalled-repo", 3)
        do $$$AssertFailure("A stalled trust server answered for the second repository")
    } catch e {
        set text = $system.Status.GetErrorText(e.AsStatus())
        do $$$AssertTrue(text [ "was not attempted because trust server " _ server _ " did not answer within 3 seconds", text)
    }
    do $$$AssertTrue(($zhorolog - start) < 2, "The second repository failed after " _ ($zhorolog - start) _ "s")
}

/// A protected repo only downloads and checks the package archive. The ORAS pull of an unprotected repo
/// unpacks a directory layer before IPM checks the package, which shows the check below can see unpacking.
Method TestNothingUnpackedBeforeVerification()
{
    if '..Available() {
        return
    }
    do ..Publish("trust-app-1.0.0", "trust-app")
    set marker = "content-trust-marker-" _ $job _ "-" _ $translate($ztimestamp, ",.")
    do $$$AssertEquals(..PushManifestWithDirectoryLayer(..#Registry, ..CaseNamespace _ "/trust-app", "1.0.0", "1.0.3", marker), 201, "Pushed a manifest with a directory layer")
    do ..Sign("trust-app", "1.0.3")

    set sc = ..Run("install trusted/trust-app 1.0.3", .lines)
    do ..AssertRefused(sc, .lines, "does not match its signed descriptor")
    set tempRoot = ##class(%File).ParentDirectoryName(##class(%IPM.Utils.File).CreateTempDirectory())
    do $$$AssertEquals(..FindDirectory(tempRoot, marker), "", "The protected repo unpacked no layer")
    set extracted = ##class(%File).NormalizeDirectory("trust-app/1.0.3", ##class(%IPM.Repo.UniversalSettings).GetModuleRoot())
    do $$$AssertNotTrue(##class(%File).DirectoryExists(extracted), "IPM did not extract the package into " _ extracted)
    do $$$AssertEquals(..FindFile(tempRoot, "trust-app/1.0.3/package.tgz"), "", "The refused package was deleted")

    do $$$AssertStatusOK(##class(%IPM.Main).Shell("repo -o -name trusted -trust-server """""), "Turned verification off")
    set sc = ..Run("install trusted/trust-app 1.0.3", .lines)
    do $$$AssertStatusNotOK(sc, "The unprotected repo also refuses the package size")
    do $$$AssertNotEquals(..FindDirectory(tempRoot, marker), "", "but only after ORAS unpacked the directory layer")
    do ..AssertNotInstalled("trust-app")
}

/// After IPM has pinned a repository's root key, trust data under another root key is refused,
/// including by reinstall with -force and by a developer-mode install, and the installed code stays.
Method TestReplacedRootRefused()
{
    if '..Available() {
        return
    }
    do ..Publish("trust-app-1.0.0", "trust-app")
    do ..Sign("trust-app", "1.0.0")
    do $$$AssertStatusOK(..Run("install trusted/trust-app 1.0.0", .lines), "Installed 1.0.0")

    // Someone without the original keys replaces the repository's trust data
    do ..Notary(..SignerDirectory, ["delete", (..GUN("trust-app")), "--remote"])
    do ..Sign("trust-app", "1.0.0", , ..SignerDirectory _ "other-root/")

    set sc = ..Run("install trusted/trust-app 1.0.0 -force", .lines)
    do ..AssertRefused(sc, .lines, "could not rotate trust to a new trusted root")
    set sc = ..Run("install trusted/trust-app 1.0.0 -force -dev", .lines)
    do ..AssertRefused(sc, .lines, "could not rotate trust to a new trusted root")
    do ..AssertRevision("1.0.0", "1.0.0 revision 1")
}

/// A trust server that cannot be reached, or a verifier that cannot be run, refuses the install,
/// even though trust data cached by an earlier successful check is still valid.
Method TestVerifierUnavailableRefused()
{
    if '..Available() {
        return
    }
    do ..Publish("trust-app-1.0.0", "trust-app")
    do ..Sign("trust-app", "1.0.0")
    do $$$AssertStatusOK(..Run("install trusted/trust-app 1.0.0", .lines), "Installed 1.0.0")

    // Give the server that is down the trust data cached from the one that is up
    set gunPath = "tuf/" _ ..GUN("trust-app") _ "/"
    set cached = ##class(%File).NormalizeDirectory(gunPath, ##class(%IPM.Repo.Oras.ContentTrust).TrustDirectory(..#TrustServerURL))
    set copy = ##class(%File).NormalizeDirectory(gunPath, ##class(%IPM.Repo.Oras.ContentTrust).TrustDirectory(..#DownTrustServerURL))
    do $$$AssertTrue(##class(%File).CreateDirectoryChain(copy), "Created " _ copy)
    do $$$AssertTrue(##class(%File).CopyDir(cached, copy, 1), "Copied cached trust data to " _ copy)
    do $$$AssertTrue(##class(%File).Exists(copy _ "metadata/timestamp.json"), "The copy includes the cached timestamp")
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("repo -o -name trusted -trust-server " _ ..#DownTrustServerURL))
    set sc = ..Run("install trusted/trust-app 1.0.0 -force", .lines)
    do ..AssertRefused(sc, .lines, "client is offline")
    do ..AssertRevision("1.0.0", "1.0.0 revision 1")

    do $$$AssertStatusOK(##class(%IPM.Main).Shell("repo -o -name trusted -trust-server " _ ..#TrustServerURL))
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("config set NotaryExecutable /nonexistent/notary"), "Set the verifier path")
    set cookie = ""
    do ##class(%IPM.Utils.Module).BeginCaptureOutput(.cookie)
    do ##class(%IPM.Main).Shell("config list")
    do ##class(%IPM.Utils.Module).EndCaptureOutput(cookie, .lines)
    do $$$AssertTrue(..FindStringInMultiDimArray("NotaryExecutable: /nonexistent/notary", .lines), "config list shows the verifier path")
    set sc = ..Run("install trusted/trust-app 1.0.0 -force", .lines)
    do ..AssertRefused(sc, .lines, "Unable to run the Docker Content Trust verifier '/nonexistent/notary'")
    do ..AssertRevision("1.0.0", "1.0.0 revision 1")
}

// ===== Every retrieval through the repo =====

/// A dependency fetched from a protected repo must be signed too, including for a local developer load.
Method TestProtectedDependency()
{
    if '..Available() {
        return
    }
    do ..Publish("trust-child", "trust-child", 1)
    do ..Publish("trust-parent", "trust-parent")
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("uninstall trust-child"), "Uninstalled the dependency")
    do ..Publish("trust-child-2.0.0", "trust-child")
    do ..Sign("trust-parent", "1.0.0")
    // Signed but outside the parent's ^1.0.0 range
    do ..Sign("trust-child", "2.0.0")

    set sc = ..Run("install trusted/trust-parent 1.0.0", .lines)
    do ..AssertRefused(sc, .lines, "Could not find satisfactory version of trust-child", "no signature for them: 1.0.0")
    do ..AssertNotInstalled("trust-parent")
    do ..AssertNotInstalled("trust-child")

    set sc = ..Run("load " _ ..FixtureDir("trust-parent") _ " -dev", .lines)
    do ..AssertRefused(sc, .lines, "Could not find satisfactory version of trust-child")
    do ..AssertNotInstalled("trust-child")
    do ##class(%IPM.Main).Shell("uninstall trust-parent")

    do ..Sign("trust-child", "1.0.0")
    do $$$AssertStatusOK(..Run("install trusted/trust-parent 1.0.0", .lines), "Installed the parent and its signed dependency")
    do $$$AssertEquals(..InstalledVersion("trust-child"), "1.0.0")
    do $$$AssertEquals($classmethod("TrustParent.Hello", "Hello"), "child via parent")

    // Local code is not checked, so a developer load of the unsigned parent works once its dependency is signed
    do ##class(%IPM.Main).Shell("uninstall trust-parent")
    do ##class(%IPM.Main).Shell("uninstall trust-child")
    do $$$AssertStatusOK(..Run("load " _ ..FixtureDir("trust-parent") _ " -dev", .lines), "Loaded the local parent in developer mode")
    do $$$AssertEquals(..InstalledVersion("trust-child"), "1.0.0")
    do $$$AssertEquals($classmethod("TrustParent.Hello", "Hello"), "child via parent")
}

/// Update offers only signed versions, and reinstall checks the signature again.
Method TestUpdateAndReinstall()
{
    if '..Available() {
        return
    }
    do ..Publish("trust-app-1.0.0", "trust-app")
    do ..Publish("trust-app-2.0.0", "trust-app")
    do ..Sign("trust-app", "1.0.0")
    do $$$AssertStatusOK(..Run("install trusted/trust-app 1.0.0", .lines), "Installed 1.0.0")

    set sc = ..Run("update trust-app 2.0.0", .lines)
    do ..AssertRefused(sc, .lines, "no signature for them: 2.0.0")
    do ..AssertRevision("1.0.0", "1.0.0 revision 1")

    do ..Sign("trust-app", "2.0.0")
    do $$$AssertStatusOK(..Run("update trust-app", .lines), "Updated to the signed 2.0.0")
    do ..AssertRevision("2.0.0", "2.0.0")
    do ..AssertLatestHistory("update", "trust-app", 1)

    do $$$AssertStatusOK(..Run("install trusted/trust-app 2.0.0 -force", .lines), "Reinstalled the signed 2.0.0")
    do ..AssertRevision("2.0.0", "2.0.0")

    do $$$AssertStatusOK(..Run("reinstall trust-app", .lines), "reinstall fetched the signed 2.0.0 again")
    do ..AssertRevision("2.0.0", "2.0.0")
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("config set NotaryExecutable /nonexistent/notary"))
    set sc = ..Run("reinstall trust-app", .lines)
    do ..AssertRefused(sc, .lines, "Unable to run the Docker Content Trust verifier")
    do ..AssertRevision("2.0.0", "2.0.0")
}

// ===== Existing selection, naming and credentials =====

/// An explicit build must itself be signed, a request without a build selects among signed versions as usual,
/// and a platform-specific artifact for another platform stays unavailable even when it is signed.
Method TestBuildAndPlatformSelection()
{
    if '..Available() {
        return
    }
    set module = "oras-version-with-build"
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("load " _ ..GetModuleDir(module)), "Loaded " _ module)
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("publish " _ module _ " -r trusted -only"), "Published the build")
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("modver " _ module _ " 1.0.0-prerelease -force"))
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("publish " _ module _ " -r trusted -only"), "Published the version without a build")
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("uninstall " _ module))
    do ..Sign(module, "1.0.0-prerelease")

    set sc = ..Run("install trusted/" _ module _ " 1.0.0-prerelease+build", .lines)
    do ..AssertRefused(sc, .lines, "no signature for them: 1.0.0-prerelease+build")
    do ..AssertNotInstalled(module)
    do $$$AssertStatusOK(..Run("install trusted/" _ module _ " 1.0.0-prerelease", .lines), "Installed the signed version")
    do $$$AssertEquals(..InstalledVersion(module), "1.0.0-prerelease")

    // platform-filter-test 1.0.0 for this platform and 2.0.0 for another, both signed
    set platformModule = "platform-filter-test"
    set platformDir = ..GetModuleDir("platform-filter", "test", "v1")
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("load -nodev " _ platformDir), "Loaded " _ platformModule)
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("package " _ platformModule _ " -only -path " _ ##class(%IPM.Utils.File).CreateTempDirectory() _ platformModule), "Packaged for this platform")
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("publish " _ platformModule _ " -r trusted -only"), "Published for this platform")
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("uninstall " _ platformModule))
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("load -nodev " _ ..GetModuleDir("platform-filter", "test", "v2")))
    do $$$AssertStatusOK(..PublishForPlatform(platformModule, "9999.1"), "Published 2.0.0 for another platform")
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("uninstall " _ platformModule))
    do ..Sign(platformModule, "1.0.0" _ $$$OrasTagPlatformSeparator _ $$$CurrentPlatformVersion)
    do ..Sign(platformModule, "2.0.0" _ $$$OrasTagPlatformSeparator _ "9999.1")

    do $$$AssertStatusOK(..Run("install trusted/" _ platformModule, .lines), "Installed the version for this platform")
    do $$$AssertEquals(..InstalledVersion(platformModule), "1.0.0")
    do $$$AssertEquals($classmethod("PlatformFilterTest.Public", "Version"), "1.0.0")
    set sc = ..Run("update " _ platformModule _ " 2.0.0", .lines)
    do ..AssertRefused(sc, .lines, "9999.1")
    do $$$AssertEquals(..InstalledVersion(platformModule), "1.0.0")
}

/// A dependency published under its external name is verified under that registry name and installs under its own name.
Method TestExternalNameDependency()
{
    if '..Available() {
        return
    }
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("load " _ ..GetModuleDir("publish-external-name", "0.0.1")), "Loaded the dependency")
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("publish default-name -r trusted -only -use-external-name"), "Published it under its external name")
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("uninstall default-name"))

    set consumer = ..GetModuleDir("oras-dep-name", "consumer-name")
    set sc = ..Run("load " _ consumer, .lines)
    do ..AssertRefused(sc, .lines, "Could not find satisfactory version of default-name")
    do ..AssertNotInstalled("default-name")
    do ##class(%IPM.Main).Shell("uninstall oras-dep-consumer-name")

    do ..Sign("external-name", "0.0.1")
    do $$$AssertStatusOK(..Run("load " _ consumer, .lines), "Loaded the consumer with its signed dependency")
    do $$$AssertEquals(..InstalledVersion("default-name"), "0.0.1")
}

/// Verification reads the registry with the repo's credentials, and errors do not show them.
Method TestBasicAuthentication()
{
    if '..Available() {
        return
    }
    set ..CaseNamespace = "auth-test/" _ ..RunNamespace
    do ##class(%IPM.Main).Shell("repo -delete -name trusted")
    do ..ConfigureTrustedRepo(..CaseNamespace, "-username writer -password writerpass")
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("load " _ ..GetModuleDir("oras-auth")), "Loaded oras-auth")
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("publish oras-auth -r trusted -only"), "Published with credentials")
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("uninstall oras-auth"))

    set sc = ..Run("install trusted/oras-auth 1.0.0", .lines)
    do ..AssertRefused(sc, .lines, "no signature for them: 1.0.0")
    do $$$AssertNotTrue(..FindStringInMultiDimArray("writerpass", .lines), "The output does not show the password")
    do $$$AssertNotTrue($system.Status.GetErrorText(sc) [ "writerpass", "The error does not show the password")

    do ..Sign("oras-auth", "1.0.0", , , , "writer", "writerpass")
    do $$$AssertStatusOK(..Run("install trusted/oras-auth 1.0.0", .lines), "Installed with credentials")
    do $$$AssertEquals(##class(OrasAuth.Hello).Hello(), "hello")
}

// ===== Helpers =====

/// Returns whether the tests can run here, recording a skip if not.
Method Available() As %Boolean
{
    if (..Unavailable '= "") {
        do $$$AssertSkipped(..Unavailable)
        return 0
    }
    return 1
}

/// Configures repo "trusted" on registry namespace with the trust server, plus any extra repo modifiers.
Method ConfigureTrustedRepo(
	namespace As %String,
	extra As %String = "")
{
    set command = "repo -o -name trusted -url " _ ..#OrasURL _ " -namespace " _ namespace _ " -publish 1 -trust-server " _ ..#TrustServerURL _ " " _ extra
    do $$$AssertStatusOK(##class(%IPM.Main).Shell(command), "Configured protected repo on " _ namespace)
}

ClassMethod FixtureDir(folder As %String) As %String
{
    return ..GetModuleDir(..#FixtureRoot, folder)
}

/// Loads fixture folder, publishes module to the protected repo with the ordinary publish command and,
/// unless keepInstalled is set, uninstalls it again.
Method Publish(
	folder As %String,
	module As %String,
	keepInstalled As %Boolean = 0)
{
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("load " _ ..FixtureDir(folder)), "Loaded " _ folder)
    do $$$AssertStatusOK(##class(%IPM.Main).Shell("publish " _ module _ " -r trusted -only"), "Published " _ folder)
    if 'keepInstalled {
        do $$$AssertStatusOK(##class(%IPM.Main).Shell("uninstall " _ module), "Uninstalled " _ module)
    }
}

/// Publishes the loaded module for platformVersion, which the publish command cannot choose.
Method PublishForPlatform(
	module As %String,
	platformVersion As %String) As %Status
{
    set sc = $$$OK
    try {
        set tgz = ##class(%IPM.Utils.File).CreateTempDirectory() _ module
        $$$ThrowOnError(##class(%IPM.Main).Shell("package " _ module _ " -only -path " _ tgz))
        set loaded = ##class(%IPM.Storage.Module).NameOpen(module, , .sc)
        $$$ThrowOnError(sc)
        set repo = ##class(%IPM.Repo.Oras.Definition).ServerDefinitionKeyOpen("trusted")
        set info = ##class(%IPM.Repo.Remote.ModuleInfo).%New()
        set info.Name = module
        set info.VersionString = loaded.VersionString
        set info.Deployed = 1
        set info.PlatformVersion = platformVersion
        set payload = ##class(%Stream.FileBinary).%New()
        set payload.Filename = tgz _ ".tgz"
        $$$ThrowOnError(info.Payload.CopyFrom(payload))
        set manifest = ##class(%Stream.GlobalCharacter).%New()
        $$$ThrowOnError(loaded.XMLExportToStream(manifest))
        set info.Manifest = manifest
        set sc = repo.GetPublishService().PublishModule(info)
    } catch e {
        set sc = e.AsStatus()
    }
    return sc
}

/// Docker Content Trust repository name of module in this test's registry namespace.
Method GUN(module As %String) As %String
{
    return ..#Registry _ "/" _ ..CaseNamespace _ "/" _ module
}

/// Signs tag of module as an external promotion process would: reads the manifest's digest and size from the
/// registry and signs them on the trust server with notary, in role if given. sizeOffset adds to the signed size.
/// Returns the digest of the manifest read.
Method Sign(
	module As %String,
	tag As %String,
	role As %String = "",
	signerDirectory As %String = {..SignerDirectory},
	sizeOffset As %Integer = 0,
	username As %String = "",
	password As %String = "") As %String
{
    set manifest = {}.%FromJSON(..ReadManifest(..#Registry, ..CaseNamespace _ "/" _ module, tag, username, password))
    set gun = ..GUN(module)
    do ..InitTrust(gun, signerDirectory)
    set args = ["addhash", (gun), (tag), (manifest.size + sizeOffset), "--sha256", ($piece(manifest.digest, ":", 2)), "-p"]
    if (role '= "") {
        do args.%Push("-r").%Push(role)
    }
    do ..Notary(signerDirectory, args)
    return manifest.digest
}

/// Creates trust data for repository gun on the trust server, with keys in signerDirectory, unless it has some.
Method InitTrust(
	gun As %String,
	signerDirectory As %String = {..SignerDirectory})
{
    if '..Notary(signerDirectory, ["list", (gun)], 1) {
        do ..Notary(signerDirectory, ["init", (gun)])
        do ..Notary(signerDirectory, ["key", "rotate", (gun), "snapshot", "-r"])
        do ..Notary(signerDirectory, ["publish", (gun)])
    }
}

/// Adds delegation role for module's repository with a new key that the signer holds.
Method AddDelegation(
	module As %String,
	role As %String)
{
    set name = $piece(role, "/", *)
    set key = ..SignerDirectory _ name _ "-key.pem"
    set cert = ..SignerDirectory _ name _ ".pem"
    set out = ##class(%Stream.TmpCharacter).%New()
    do $$$AssertStatusOK(##class(%IPM.Utils.Module).RunCommand(..SignerDirectory, $listbuild("openssl", "ecparam", "-name", "prime256v1", "-genkey", "-noout", "-out", key), .out, .out), "Generated a key for " _ role)
    do $$$AssertStatusOK(##class(%IPM.Utils.Module).RunCommand(..SignerDirectory, $listbuild("openssl", "req", "-new", "-x509", "-key", key, "-out", cert, "-days", "3650", "-subj", "/CN=" _ name), .out, .out), "Generated a certificate for " _ role)
    do ..Notary(..SignerDirectory, ["delegation", "add", (..GUN(module)), (role), (cert), "--all-paths", "-p"])
    do ..Notary(..SignerDirectory, ["key", "import", (key), "--role", (role)])
}

/// Runs notary as the signer, with keys in signerDirectory. Returns whether it succeeded if
/// mayFail is set, and otherwise asserts that it did.
Method Notary(
	signerDirectory As %String,
	args As %DynamicArray,
	mayFail As %Boolean = 0) As %Boolean
{
    set result = {}.%FromJSON(..RunSigner(##class(%IPM.Repo.Oras.ContentTrust).NotaryExecutable(), signerDirectory, ..#TrustServerURL, args.%ToJSON()))
    if mayFail {
        return result.ok
    }
    do $$$AssertTrue(result.ok, "notary " _ args.%Get(0) _ ": " _ result.output)
    return result.ok
}

/// Runs notary for the signer with its passphrases, returning {"ok": &lt;succeeded&gt;, "output": &lt;stdout and stderr&gt;}.
ClassMethod RunSigner(
	notary As %String,
	signerDirectory As %String,
	trustServer As %String,
	argsJSON As %String) As %String [ Language = python ]
{
    import json
    import os
    import subprocess

    env = dict(os.environ)
    env.update({
        "NOTARY_ROOT_PASSPHRASE": "ipm-test-root",
        "NOTARY_TARGETS_PASSPHRASE": "ipm-test-targets",
        "NOTARY_SNAPSHOT_PASSPHRASE": "ipm-test-snapshot",
        "NOTARY_DELEGATION_PASSPHRASE": "ipm-test-delegation",
    })
    command = [notary, "-d", signerDirectory, "-s", trustServer] + [str(arg) for arg in json.loads(argsJSON)]
    process = subprocess.run(command, env=env, capture_output=True, text=True, timeout=120)
    return json.dumps({"ok": process.returncode == 0, "output": process.stdout + process.stderr})
}

/// Returns {"digest": "sha256:...", "size": n} of the manifest that tag names in repository path.
ClassMethod ReadManifest(
	registry As %String,
	path As %String,
	tag As %String,
	username As %String = "",
	password As %String = "") As %String [ Language = python ]
{
    import hashlib
    import json
    import requests

    url = f"http://{registry}/v2/{path}/manifests/{tag}"
    auth = (username, password) if username else None
    response = requests.get(url, headers={"Accept": "application/vnd.oci.image.manifest.v1+json"}, auth=auth, timeout=30)
    response.raise_for_status()
    return json.dumps({"digest": "sha256:" + hashlib.sha256(response.content).hexdigest(), "size": len(response.content)})
}

/// Returns the digest of the manifest tag names in module's repository in this test's namespace.
Method ManifestDigest(
	module As %String,
	tag As %String) As %String
{
    return {}.%FromJSON(..ReadManifest(..#Registry, ..CaseNamespace _ "/" _ module, tag)).digest
}

/// Returns the digest of the package layer in the manifest with manifestDigest in module's repository in this test's namespace.
Method PackageDigest(
	module As %String,
	manifestDigest As %String) As %String
{
    set request = ##class(%Net.HttpRequest).%New()
    do ##class(%Net.URLParser).Decompose(..#OrasURL, .parts)
    set request.Server = parts("host")
    set request.Port = parts("port")
    do request.SetHeader("Accept", "application/vnd.oci.image.manifest.v1+json")
    $$$ThrowOnError(request.Get("/v2/" _ ..CaseNamespace _ "/" _ module _ "/manifests/" _ manifestDigest))
    return {}.%FromJSON(request.HttpResponse.Data).layers.%Get(0).digest
}

/// Returns "sha256:" and the hex SHA-256 of stream.
ClassMethod StreamDigest(stream As %Stream.Object) As %String
{
    do stream.Rewind()
    return "sha256:" _ $zconvert(##class(%xsd.hexBinary).LogicalToXSD($system.Encryption.SHAHashStream(256, stream)), "L")
}

/// Deletes the manifest with digest from module's repository in this test's namespace.
Method DeleteManifest(
	module As %String,
	digest As %String)
{
    do $$$AssertEquals(..RegistryRequest(..#Registry, "DELETE", ..CaseNamespace _ "/" _ module, digest), 202, "Deleted manifest " _ digest)
}

/// Pushes as newTag a copy of the manifest tag names in module's repository, changed so that its package layer
/// is one byte longer than the package ("size"), appears twice ("duplicate") or names the config blob ("config").
Method PushModifiedManifest(
	module As %String,
	tag As %String,
	newTag As %String,
	change As %String)
{
    do $$$AssertEquals(..RegistryRequest(..#Registry, "PUT", ..CaseNamespace _ "/" _ module, newTag, tag, change), 201, "Pushed modified manifest " _ newTag)
}

/// Sends method to the manifest of reference in repository path and returns the status code.
/// For PUT, sends the manifest that sourceTag names, changed as PushModifiedManifest describes.
ClassMethod RegistryRequest(
	registry As %String,
	method As %String,
	path As %String,
	reference As %String,
	sourceTag As %String = "",
	change As %String = "") As %Integer [ Language = python ]
{
    import json
    import requests

    base = f"http://{registry}/v2/{path}/manifests/"
    mediaType = "application/vnd.oci.image.manifest.v1+json"
    if method != "PUT":
        return requests.request(method, base + reference, timeout=30).status_code
    manifest = requests.get(base + sourceTag, headers={"Accept": mediaType}, timeout=30).json()
    layer = manifest["layers"][0]
    if change == "size":
        layer["size"] += 1
    elif change == "config":
        layer["digest"] = manifest["config"]["digest"]
        layer["size"] = manifest["config"]["size"]
    else:
        manifest["layers"].append(dict(layer))
    return requests.put(base + reference, data=json.dumps(manifest), headers={"Content-Type": mediaType}, timeout=30).status_code
}

/// Pushes as newTag a copy of the manifest sourceTag names in repository path, with its package layer one byte
/// longer than the package and an extra directory layer that ORAS unpacks into a directory named marker.
ClassMethod PushManifestWithDirectoryLayer(
	registry As %String,
	path As %String,
	sourceTag As %String,
	newTag As %String,
	marker As %String) As %Integer [ Language = python ]
{
    import hashlib
    import io
    import json
    import requests
    import tarfile

    base = f"http://{registry}/v2/{path}"
    mediaType = "application/vnd.oci.image.manifest.v1+json"
    manifest = requests.get(f"{base}/manifests/{sourceTag}", headers={"Accept": mediaType}, timeout=30).json()
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w:gz") as archive:
        content = b"unpacked by the registry client\n"
        entry = tarfile.TarInfo(f"{marker}/marker.txt")
        entry.size = len(content)
        archive.addfile(entry, io.BytesIO(content))
    blob = buffer.getvalue()
    digest = "sha256:" + hashlib.sha256(blob).hexdigest()
    location = requests.post(f"{base}/blobs/uploads/", timeout=30).headers["Location"]
    if location.startswith("/"):
        location = f"http://{registry}{location}"
    separator = "&" if "?" in location else "?"
    requests.put(f"{location}{separator}digest={digest}", data=blob, headers={"Content-Type": "application/octet-stream"}, timeout=30).raise_for_status()
    manifest["layers"][0]["size"] += 1
    manifest["layers"].insert(0, {
        "mediaType": "application/vnd.oci.image.layer.v1.tar+gzip",
        "digest": digest,
        "size": len(blob),
        "annotations": {"org.opencontainers.image.title": marker, "io.deis.oras.content.unpack": "true"},
    })
    return requests.put(f"{base}/manifests/{newTag}", data=json.dumps(manifest), headers={"Content-Type": mediaType}, timeout=30).status_code
}

/// Returns the path of a directory named name at most four levels below root, or "" if there is none.
ClassMethod FindDirectory(
	root As %String,
	name As %String) As %String [ Language = python ]
{
    import os

    depth = root.rstrip(os.sep).count(os.sep)
    for current, directories, files in os.walk(root):
        if name in directories:
            return os.path.join(current, name)
        if current.count(os.sep) - depth >= 4:
            directories[:] = []
    return ""
}

/// Returns the tags in repository path as a comma separated string, or "" if it has none.
/// Uses the credentials of repo "trusted", since the auth-test namespace requires them.
Method PublishedTags(path As %String) As %String
{
    set repo = ##class(%IPM.Repo.Oras.Definition).ServerDefinitionKeyOpen("trusted")
    set client = ##class(%IPM.Repo.Oras.PackageService).GetClient(..#OrasURL, repo.Username, repo.Password, repo.Token, repo.TokenAuthMethod)
    return ##class(%IPM.Repo.Oras.PackageService).GetAllTags(..#OrasURL, path, "", client)
}

/// Starts the stand-in HTTP server in mode on port for seconds, and waits until it answers.
Method StartStub(
	mode As %String,
	port As %Integer,
	seconds As %Integer)
{
    set script = ..FixtureDir("stub") _ "server.py"
    set null = ##class(%Library.Device).GetNullDevice()
    do $zf(-100, "/ASYNC /STDOUT=""" _ null _ """ /STDERR=""" _ null _ """", "python3", script, mode, port, seconds)
    set request = ##class(%Net.HttpRequest).%New()
    set request.Server = "127.0.0.1"
    set request.Port = port
    set request.Timeout = 1
    for i = 1:1:50 {
        if $$$ISOK(request.Get("/v2/")) {
            return
        }
        hang 0.2
    }
    do $$$AssertFailure("The stand-in server on port " _ port _ " did not start")
}

/// Returns the path of a file whose path ends in relative at most six levels below root, or "" if there is none.
ClassMethod FindFile(
	root As %String,
	relative As %String) As %String [ Language = python ]
{
    import os

    depth = root.rstrip(os.sep).count(os.sep)
    for current, directories, files in os.walk(root):
        for name in files:
            path = os.path.join(current, name)
            if path.endswith(os.sep + relative):
                return path
        if current.count(os.sep) - depth >= 6:
            directories[:] = []
    return ""
}

/// Runs a shell command, returning its status and its output in lines.
Method Run(
	command As %String,
	Output lines) As %Status
{
    kill lines
    set cookie = ""
    do ##class(%IPM.Utils.Module).BeginCaptureOutput(.cookie)
    set sc = ##class(%IPM.Main).Shell(command)
    do ##class(%IPM.Utils.Module).EndCaptureOutput(cookie, .lines)
    return sc
}

/// Asserts that sc failed and that its error text or the command's output contains each substring.
Method AssertRefused(
	sc As %Status,
	ByRef lines,
	substrings... As %String)
{
    do $$$AssertStatusNotOK(sc, "The command was refused")
    set text = $system.Status.GetErrorText(sc)
    for i = 1:1:$get(lines) {
        set text = text _ $char(10) _ $get(lines(i))
    }
    for i = 1:1:$get(substrings) {
        do $$$AssertTrue(text [ substrings(i), "Refusal mentions: " _ substrings(i) _ $char(10) _ "Output was: " _ text)
    }
}

/// Asserts that trust-app is installed at version and its code reports revision.
Method AssertRevision(
	version As %String,
	revision As %String)
{
    do $$$AssertEquals(..InstalledVersion("trust-app"), version, "trust-app " _ version _ " is installed")
    do $$$AssertEquals($classmethod("TrustApp.Hello", "Revision"), revision, "The installed code is " _ revision)
}

Method AssertNotInstalled(module As %String)
{
    do $$$AssertEquals(..InstalledVersion(module), "", module _ " is not installed")
}

/// Asserts that the newest history entry is action on module, successful or not.
Method AssertLatestHistory(
	action As %String,
	module As %String,
	success As %Boolean)
{
    set rs = ##class(%IPM.General.History).GetHistory(, 0, 1)
    if '$$$AssertTrue(rs.%Next(), "There is a history entry") {
        return
    }
    do $$$AssertEquals(rs.%Get("Action"), action, "The newest history entry is " _ action)
    do $$$AssertEquals(rs.%Get("Package"), module, "The newest history entry is for " _ module)
    do $$$AssertEquals(''$$$ISOK(rs.%Get("Success")), success, "The newest history entry's success")
}

/// Version of the named module as installed, or "" if it is not installed.
ClassMethod InstalledVersion(module As %String) As %String
{
    set loaded = ##class(%IPM.Storage.Module).NameOpen(module, , .sc)
    return $select($$$ISERR(sc) || '$isobject(loaded): "", 1: loaded.VersionString)
}

/// Returns whether the trust server answers its health check.
ClassMethod TrustServerHealthy() As %Boolean
{
    set request = ##class(%Net.HttpRequest).%New()
    do ##class(%Net.URLParser).Decompose(..#TrustServerURL, .parts)
    set request.Server = parts("host")
    set request.Port = $get(parts("port"))
    set request.Timeout = 5
    return $$$ISOK(request.Get("/_notary_server/health")) && (request.HttpResponse.StatusCode = 200)
}

}
