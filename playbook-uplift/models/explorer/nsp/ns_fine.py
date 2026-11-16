"""Fine L2: IPM namespace commands at 571e04a1 over the IRIS mapping contract (nsenv), close to the ObjectScript.

Unlike the Verus model (ns_mapping.rs), this keeps the literal ordered list loops, the $namespace switches, each
Config call with its argument slots, the per-call activations and the printed lines. CODE_MUTANTS are broken
implementations; each must be rejected by exploration (L1 rules) or by conformance with IRIS.
"""
from __future__ import annotations

from nsenv import LIB, REPO_GLOBALS, Iris

CODE_MUTANTS = {
    "unmap-routine-fixed": "UnmapIPM passes the routine type ALL (the F-NSP-6 fix; IRIS contradicts it)",
    "repo-loop-after-activation": "the repo loop runs after batch activation (the F-NSP-4 fix; IRIS contradicts it)",
    "enable-skips-mapped": "enable -map also skips a namespace that already reaches IPM by mapping",
    "repo-loop-no-rows-check": "enable -map -repos maps over a namespace's own registries (Main.cls#L3675 dropped)",
    "unmap-keeps-package": "UnmapIPM does not remove the %IPM package mapping",
    "installer-no-mapping": "the installer adds no %IPM mapping",
    "mapinfo-first-source": "GetMapInfo keeps the first source sharing a database, not the last",
    "no-batch-activation": "EnableIPM never calls PerformBatchActivation",
}


# ---------------------------------------------------------------- Utils/Module.cls
def activate_mappings(env: Iris, ns):                                  # #L1869
    env.activate(ns)


def add_package_mapping(env, ns, frm, skip=0):                          # #L1614-L1644
    if not env.exists(ns, "pkg"):
        env.create(ns, "pkg", frm)
        if not skip:
            activate_mappings(env, ns)


def remove_package_mapping(env, ns, skip=0):                            # #L1646-L1671
    if env.exists(ns, "pkg"):
        env.delete(ns, "pkg")
        if not skip:
            activate_mappings(env, ns)
        return 1
    return 0


def rtn_key(typ):
    return "rtn:ALL" if typ == "ALL" else f"rtn:{typ}"


def add_routine_mapping(env, ns, typ, frm, skip=0):                     # #L1673-L1705
    k = rtn_key(typ)
    if not env.exists(ns, k):
        env.create(ns, k, frm)
        if not skip:
            activate_mappings(env, ns)


def remove_routine_mapping(env, ns, typ="ALL", skip=0):                 # #L1707-L1734
    k = "rtn:ALL" if "routine-type-ignored" in env.mut else rtn_key(typ)
    if env.exists(ns, k):
        env.delete(ns, k)
        if not skip:
            activate_mappings(env, ns)
        return 1
    return 0


def add_global_mapping(env, ns, g, frm, set_collation=0, skip=0):       # #L1736-L1781
    k = "glo:" + g
    if not env.exists(ns, k):
        env.create(ns, k, frm)
        if not skip:
            activate_mappings(env, ns)


def remove_global_mapping(env, ns, g, skip=0):                          # #L1783
    k = "glo:" + g
    if env.exists(ns, k):
        env.delete(ns, k)
        if not skip:
            activate_mappings(env, ns)
        return 1
    return 0


def perform_batch_activation(env, nss):                                 # #L1888-L1918
    for ns in sorted(nss):
        if ns in env.cfg:
            env.activate(ns)


# ---------------------------------------------------------------- Utils/Build.cls
def map_package_equivalently(env, src, to, skip=0):                     # #L537-L567
    add_package_mapping(env, to, env.pkg_dest(src), skip)


def map_routine_equivalently(env, src, to, skip=0):                     # #L570-L601
    add_routine_mapping(env, to, "ALL", env.rtn_dest(src), skip)


def map_global_equivalently(env, g, src, to, skip=0):                   # #L604-L634
    if "global-map-honours-skip" in env.mut:
        add_global_mapping(env, to, g, env.glo_dest(src, g), 0, skip)
    else:
        add_global_mapping(env, to, g, env.glo_dest(src, g), skip)     # #L628: skip lands in pSetCollation


# ---------------------------------------------------------------- Main.cls
def ipm_installed(env, ns):                                             # #L4122
    return env.has_ipm(ns) and env.zpm_record(ns) is not None


def get_map_info(env, mut=frozenset()):                                 # #L4106-L4120, #L1628-L1654
    sources = [ns for ns in env.namespaces() if env.has_ipm(ns) and env.zpm_record(ns) is not None]
    by_db = {}
    for ns in sources:
        d = env.pkg_dest(ns)
        if "mapinfo-first-source" in mut and d in by_db:
            continue
        by_db[d] = ns
    return {ns: by_db[env.pkg_dest(ns)] for ns in env.namespaces() if ns not in sources and env.pkg_dest(ns) in by_db}


def enable_map(env: Iris, init, namespaces, repos=False, quiet=False, preview=False, mut=frozenset()):
    """EnableIPM with -map (#L3611-L3708). namespaces: the -ns list (upper-cased), or every namespace for -globally."""
    p = env.out.append
    to_activate = set()
    for ns in namespaces:                                               # #L3628
        ns = ns.strip()
        if ipm_installed(env, ns) or ("enable-skips-mapped" in mut and env.has_ipm(ns)):
            if not quiet or preview:
                p(f"Skipping IPM mapping of {ns} - IPM already installed.")
            continue
        if preview:
            p(f"Would add IPM mappings to {ns}")
            continue
        if not quiet:
            p(f"Mapping %IPM package in {init} equivalently to {ns}")
        map_package_equivalently(env, init, ns, 1)
        to_activate.add(ns)
        if not quiet:
            p(f"Mapping %IPM.* routines in {init} equivalently to {ns}")
        map_routine_equivalently(env, init, ns, 1)

    def repo_loop():
        info = get_map_info(env, mut)                                   # #L3654
        for ns in namespaces:
            ns = ns.strip()
            if info.get(ns) != init:
                if not quiet or preview:
                    p(f"Skipping repository mapping of {ns} - IPM not mapped from source namespace.")
                continue
            if env.repo_rows(ns) and "repo-loop-no-rows-check" not in mut:
                if not quiet or preview:
                    p(f"Skipping repository mapping of {ns} - IPM repositories found.")
                continue
            if preview:
                p(f"Would add IPM repository mappings to {ns}")
                continue
            if not quiet:
                p(f"Mapping IPM repository in {init} equivalently to {ns}")
            to_activate.add(ns)
            for g in REPO_GLOBALS:
                map_global_equivalently(env, g, init, ns, 1)

    if repos and "repo-loop-after-activation" not in mut:
        repo_loop()
    if not preview and to_activate and "no-batch-activation" not in mut:
        perform_batch_activation(env, to_activate)                      # #L3694-L3698
    if repos and "repo-loop-after-activation" in mut:
        to_activate = set()
        repo_loop()
        perform_batch_activation(env, to_activate)
    if preview:
        p("Preview mode; no configuration changes were made.")


def repo_delete_all(env, ns):
    env.rows[env.glo_dest(ns)] = []


def repo_reset_defaults(env, ns):
    env.rows[env.glo_dest(ns)] = ["registry"]


def community(env, init, fail=(False, False, False), mut=frozenset()):
    """enable -community (#L3564-L3575): three commands; a failing one prints an error and the next runs."""
    for i, step in enumerate(("repo -delete-all", "repo -reset-defaults", "enable -map -repos -globally")):
        env.out.append(f'RUNNING command: "{step}"')
        if fail[i]:
            env.out.append(f'Error running command "{step}"')
            continue
        if i == 0:
            repo_delete_all(env, init)
        elif i == 1:
            repo_reset_defaults(env, init)
        else:
            enable_map(env, init, [n for n in env.namespaces() if n != "%ALL"], repos=True, mut=mut)
    return "ok"


def unmap(env: Iris, namespaces, repos_only=False, mut=frozenset()):    # #L3927-L4004
    p = env.out.append
    p(("Will attempt to unmap IPM repository settings from: " if repos_only else
       "Will attempt to unmap IPM package, routines, and repository settings from: ") + ", ".join(namespaces))
    info = get_map_info(env, mut)
    to_activate = set()
    for ns in namespaces:
        src = info.get(ns, "")
        if src == "":
            p(f"No mapping found for {ns}. Skipping.")
            continue
        p(f"Unmapping repository settings from {ns} (mapped from {src})")
        for g in REPO_GLOBALS:
            if remove_global_mapping(env, ns, g, 1):
                to_activate.add(ns)
        if not repos_only:
            p(f"Unmapping IPM package and routines from {ns} (mapped from {src})")
            if "unmap-keeps-package" not in mut and remove_package_mapping(env, ns, 1):
                to_activate.add(ns)
            typ = "ALL" if "unmap-routine-fixed" in mut else 1              # #L3998: 1 lands in pType
            if remove_routine_mapping(env, ns, typ, 1 if typ == "ALL" else 0):
                to_activate.add(ns)
    if to_activate:
        perform_batch_activation(env, to_activate)


def installer(env: Iris, ns, version, mut=frozenset()):
    """IPM.Installer setup (Installer.cls#L25-L39, %Installer Configuration then Import, ZPMLoad)."""
    if "installer-no-mapping" not in mut:
        for key in ("pkg", "rtn:ALL"):
            if "installer-repoints" in env.mut or not env.exists(ns, key):
                env.create(ns, key, ns)
        env.activate(ns)
    d = env.pkg_dest(ns)
    if d != LIB:
        env.code.add(d)
    env.zpm[ns] = version
    return "ok"


def install_zpm(env: Iris, ns, version, mut=frozenset()):
    """`install zpm` in ns: CheckModuleNamespace (#L2534-L2540), then a module load of IPM itself."""
    info = get_map_info(env, mut)
    if ns in info:
        return f"err:ERROR #5001: Cannot install 'zpm' in namespace '{ns}' because it is mapped from namespace '{info[ns]}'."
    d = env.pkg_dest(ns)
    if d != LIB:
        env.code.add(d)
    env.zpm[ns] = version
    return "ok"


def check_ipm_version(env: Iris, ns):
    """SystemRequirements.CheckIPMVersion (#L91-L95): the zpm record of the current namespace, or ""."""
    return env.zpm_record(ns) or ""


def enable_copy_registry(env: Iris, ns, install_ok):
    """EnableIPM without -map, registry path (#L3852-L3886): the LoadStream status is discarded (F-NSP-1)."""
    if install_ok:
        installer(env, ns, "9.9.9")
    env.out.append(f"IPM enabled for namespace {ns}")
    return "ok"
