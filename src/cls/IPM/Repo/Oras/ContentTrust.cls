Include %IPM.Common

/// Checks ORAS artifacts against a Docker Content Trust server, the Notary service that docker trust signs to.
/// The notary executable verifies a repository's signed TUF metadata. A tag is trusted when it is signed in the
/// targets/releases role, or failing that in the targets role, which are the roles docker pull accepts.
/// <p>
/// Trust data is kept per trust server under <method>TrustDirectory</method>. As with docker, the first lookup of
/// a repository pins the root key the server presents, and a later root must be signed by the pinned one.
/// To trust a new root key for a repository, delete its folder under tuf/ in that directory.
/// Root keys can also be pinned in advance with trust_pinning in the notary client configuration of the IRIS user.
Class %IPM.Repo.Oras.ContentTrust
{

/// Roles whose targets are trusted, highest priority first.
Parameter TRUSTEDROLES = "targets/releases,targets";

/// Seconds notary may take to check a repository before the check fails.
Parameter VERIFYTIMEOUT = 120;

/// Returns {"digest": "sha256:&lt;hex&gt;", "size": &lt;bytes&gt;, "role": &lt;role&gt;} for the manifest that
/// trustServer signed as tag of repository, e.g. repository "registry.example.com/ns/module".
/// Throws if tag is not signed in a trusted role or the trust data cannot be verified.
/// See <method>SignedTargets</method> for repoName.
ClassMethod TrustedTarget(
	trustServer As %String,
	repository As %String,
	tag As %String,
	repoName As %String = "") As %DynamicObject
{
    set target = ..SignedTargets(trustServer, repository, repoName).%Get(tag)
    if '$isobject(target) {
        set msg = $$$FormatText("No trust data for '%1:%2' on trust server %3. Only tags signed in the targets/releases or targets role can be installed from this repository.", repository, tag, trustServer)
        $$$ThrowStatus($$$ERROR($$$GeneralError, msg))
    }
    return target
}

/// Returns {&lt;tag&gt;: &lt;descriptor as in TrustedTarget&gt;, ...} for every tag of repository signed in a trusted role,
/// or {} if trustServer has no trust data for repository.
/// Throws if the trust data cannot be verified, or notary cannot be run or cannot reach trustServer.
/// Within a shell command, the outcome is kept for repo repoName until the command ends, so the version an install
/// selects, the manifest its dependencies are read from and the package it loads are the same signed artifact.
/// Once trustServer has not answered in time, other repositories checked on it in the same command fail at once,
/// rather than each waiting for timeout seconds.
ClassMethod SignedTargets(
	trustServer As %String,
	repository As %String,
	repoName As %String = "",
	timeout As %Integer = {..#VERIFYTIMEOUT}) As %DynamicObject
{
    set keep = (repoName '= "") && $data($$$RepoLookupState)
    set outcome = ""
    if keep {
        set outcome = $get($$$RepoLookupState(repoName, "trust", trustServer, repository))
        if (outcome = "") && ($data($$$RepoLookupState(repoName, "trust", trustServer), stalled) # 2) {
            set msg = $$$FormatText("Content trust verification of '%1' was not attempted because trust server %2 did not answer within %3 seconds earlier in this command.", repository, trustServer, stalled)
            set outcome = $listbuild(0, $$$ERROR($$$GeneralError, msg))
        }
    }
    if (outcome = "") {
        set timedOut = 0
        try {
            set outcome = $listbuild(1, ..LookUpSignedTargets(trustServer, repository, timeout, .timedOut).%ToJSON())
        } catch e {
            if e.%IsA("%Exception.SystemException") {
                throw e
            }
            set outcome = $listbuild(0, e.AsStatus())
        }
        if keep {
            set $$$RepoLookupState(repoName, "trust", trustServer, repository) = outcome
            if timedOut {
                set $$$RepoLookupState(repoName, "trust", trustServer) = timeout
            }
        }
    }
    if '$listget(outcome, 1) {
        $$$ThrowStatus($listget(outcome, 2))
    }
    return {}.%FromJSON($listget(outcome, 2))
}

/// Runs notary to list the tags of repository signed in a trusted role, as SignedTargets returns them.
/// Fails, setting timedOut, if notary takes more than timeout seconds.
ClassMethod LookUpSignedTargets(
	trustServer As %String,
	repository As %String,
	timeout As %Integer = {..#VERIFYTIMEOUT},
	Output timedOut As %Boolean = 0) As %DynamicObject [ Internal ]
{
    set timedOut = 0
    // The name comes from the registry and becomes part of the cached timestamp's path below
    if '$match(repository, "[a-z0-9.-]+(:[0-9]+)?(/[a-z0-9]+(([._]|__|-+)[a-z0-9]+)*)+") {
        $$$ThrowStatus($$$ERROR($$$GeneralError, $$$FormatText("'%1' is not a valid OCI repository name.", repository)))
    }
    set trustDir = ..TrustDirectory(trustServer)
    // notary falls back to cached trust data when it cannot reach the server, until the cached timestamp expires.
    // Without a cached timestamp it has to download a current one, so an unreachable server fails the lookup.
    set timestamp = ##class(%File).NormalizeFilename("tuf/" _ repository _ "/metadata/timestamp.json", trustDir)
    if ##class(%File).Exists(timestamp) && '##class(%File).Delete(timestamp, .deleteError) {
        $$$ThrowStatus($$$ERROR($$$GeneralError, $$$FormatText("Unable to remove cached trust timestamp '%1': error %2", timestamp, deleteError)))
    }

    set executable = ..NotaryExecutable()
    set command = $listbuild(executable, "-d", trustDir, "-s", trustServer, "list", repository, "-r", ..#TRUSTEDROLES)
    set stdout = ##class(%Stream.TmpCharacter).%New()
    set stderr = ##class(%Stream.TmpCharacter).%New()
    set returnCode = ""
    // notary has no overall deadline, so a server that accepts the connection and then stalls would hang the command
    set timed = ($system.Version.GetOS() = "UNIX")
    if timed {
        set sc = ##class(%IPM.Utils.Module).RunCommand(trustDir, $listbuild("timeout", "-k", 10, timeout) _ command, .stdout, .stderr, .returnCode)
        // Without the timeout program, run notary without a deadline rather than not at all
        if (returnCode = -1) {
            set timed = 0
        }
    }
    if 'timed {
        set returnCode = ""
        set sc = ##class(%IPM.Utils.Module).RunCommand(trustDir, command, .stdout, .stderr, .returnCode)
    }
    if timed && ((returnCode = 124) || (returnCode = 137)) {
        set timedOut = 1
        $$$ThrowStatus($$$ERROR($$$GeneralError, $$$FormatText("Content trust verification of '%1' against trust server %2 did not finish within %3 seconds.", repository, trustServer, timeout)))
    }
    if (returnCode = "") || (returnCode < 0) || (timed && ((returnCode = 126) || (returnCode = 127))) {
        set msg = $$$FormatText("Unable to run the Docker Content Trust verifier '%1' for trust server %2. Check that notary is installed, or set its path with: config set NotaryExecutable <path>, and that this user may run external programs.", executable, trustServer)
        $$$ThrowStatus($$$EMBEDSC($$$ERROR($$$GeneralError, msg), sc))
    }
    if (returnCode '= 0) {
        set reason = ..NotaryError(stdout, stderr, returnCode)
        if (reason [ "does not have trust data for") {
            return {}
        }
        set msg = $$$FormatText("Content trust verification of '%1' against trust server %2 failed: %3", repository, trustServer, reason)
        $$$ThrowStatus($$$ERROR($$$GeneralError, msg))
    }
    // notary prints target names as they are, so a name with whitespace in it could pose as a row of its own
    set unsafe = ..UnsafeTargetName(##class(%File).NormalizeDirectory("tuf/" _ repository _ "/metadata", trustDir))
    if (unsafe '= "") {
        $$$ThrowStatus($$$ERROR($$$GeneralError, $$$FormatText("Trust data for '%1' on trust server %2 has a target name that is not a tag: %3", repository, trustServer, unsafe)))
    }
    return ..ParseTargets(stdout)
}

/// Returns a target name containing whitespace or a control character in the targets metadata notary has cached in
/// metadataDirectory (targets.json and every delegation below targets/), quoted, or "" if there is none.
ClassMethod UnsafeTargetName(metadataDirectory As %String) As %String [ Language = python ]
{
    import json
    import os

    files = [os.path.join(metadataDirectory, "targets.json")]
    for root, directories, names in os.walk(os.path.join(metadataDirectory, "targets")):
        files += [os.path.join(root, name) for name in names if name.endswith(".json")]
    for path in files:
        if not os.path.isfile(path):
            continue
        with open(path, encoding="utf-8") as f:
            targets = (json.load(f).get("signed") or {}).get("targets") or {}
        for name in targets:
            if any(c.isspace() or not c.isprintable() for c in name):
                return json.dumps(name)
    return ""
}

/// Returns the signed targets in listing, the output of "notary list", whose role is trusted, keeping the first row for a tag.
/// The listing looks like:
/// <pre>
/// NAME     DIGEST                                                              SIZE (BYTES)    ROLE
/// ----     ------                                                              ------------    ----
/// 1.0.0    77ab66ed470a9688624b44f2c8e342f78a50d5a2282daddc109b70c2dc3b9fb2    1004            targets
/// </pre>
ClassMethod ParseTargets(listing As %Stream.TmpCharacter) As %DynamicObject
{
    set targets = {}
    set roles = $listfromstring(..#TRUSTEDROLES)
    set listing.LineTerminator = $char(10)
    do listing.Rewind()
    while 'listing.AtEnd {
        set line = $zstrip(listing.ReadLine(), "<>WC")
        set fields = $listfromstring($zstrip(line, "=W"), " ")
        if ($listlength(fields) '= 4) {
            continue
        }
        set $listbuild(tag, hex, size, role) = fields
        // Skips the header and separator, and any row docker would not accept
        if '$listfind(roles, role) || '$match(tag, "[A-Za-z0-9_][A-Za-z0-9._-]{0,127}") || '$match(hex, "[0-9a-f]{64}") || '$match(size, "[0-9]+") {
            continue
        }
        if 'targets.%IsDefined(tag) {
            do targets.%Set(tag, {"digest": ("sha256:" _ hex), "size": (+size), "role": (role)})
        }
    }
    return targets
}

/// Returns the reason notary gave for failing with returnCode. notary prints it as "* fatal: &lt;reason&gt;" on stdout and logs to stderr.
ClassMethod NotaryError(
	stdout As %Stream.TmpCharacter,
	stderr As %Stream.TmpCharacter,
	returnCode As %Integer) As %String [ Internal ]
{
    set lastLogged = ""
    for stream = stdout, stderr {
        set stream.LineTerminator = $char(10)
        do stream.Rewind()
        while 'stream.AtEnd {
            set line = $zstrip(stream.ReadLine(), "<>WC")
            if ($extract(line, 1, 9) = "* fatal: ") {
                return $extract(line, 10, *)
            }
            if (stream = stderr) && (line '= "") {
                set lastLogged = line
            }
        }
    }
    return $select(lastLogged = "": "notary exited with code " _ returnCode, 1: lastLogged)
}

/// Returns the directory holding the trust data notary keeps for trustServer, creating it if needed.
ClassMethod TrustDirectory(trustServer As %String) As %String
{
    set safe = ""
    for i = 1:1:$length(trustServer) {
        set char = $extract(trustServer, i)
        set safe = safe _ $select(char?1AN: char, 1: "_")
    }
    set hash = $zconvert(##class(%xsd.hexBinary).LogicalToXSD($system.Encryption.SHAHash(256, $zconvert(trustServer, "O", "UTF8"))), "L")
    set dir = ##class(%File).NormalizeDirectory("ipm/content-trust/" _ safe _ "-" _ $extract(hash, 1, 12), ##class(%File).ManagerDirectory())
    if '##class(%File).DirectoryExists(dir) && '##class(%File).CreateDirectoryChain(dir, .createError) {
        $$$ThrowStatus($$$ERROR($$$GeneralError, $$$FormatText("Unable to create content trust directory '%1': error %2", dir, createError)))
    }
    return dir
}

/// Returns the notary executable configured by the NotaryExecutable setting, or "notary" to find it on the PATH.
ClassMethod NotaryExecutable() As %String
{
    set executable = ##class(%IPM.Repo.UniversalSettings).GetValue(##class(%IPM.Repo.UniversalSettings).#NotaryExecutable)
    return $select(executable = "": "notary", 1: executable)
}

}
