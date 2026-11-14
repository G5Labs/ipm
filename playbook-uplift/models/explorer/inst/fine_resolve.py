"""Fine L2 of the dependency resolver, written from the ObjectScript (ipm 571e04a1): Storage/Module.cls
BuildDependencyGraph (#L898-L1048), ProcessSingleDependencyIterative (#L1068-L1295), HasScope (#L834-L855), and
Utils/Module.cls ConstructInvertedDependencyGraph / GetFlatDependencyListFromInvertedDependencyGraph (install order)
and the load-time downgrade check (#L1213-L1216).

Structure follows the code: workQueue(depth, name) = (modulePath, scope); pDependencyGraph(name) = node and
pDependencyGraph(name, "parent version") = requirement; version expressions are sets of allowed versions and And
raises on an empty intersection ("No overlapping versions"); the repository search returns matches newest first.
"""
from __future__ import annotations


MUTANTS = {
    "oldest-first": "the registry search returns the oldest match first",
    "scoped-always": "scoped requirements count in every phase",
    "no-floor": "no '>=installed' bound is added",
}
MUT: set = set()


class ResolveError(Exception):
    def __init__(self, kind, module, sources=()):
        super().__init__(f"{kind} {module} {sources}")
        self.kind, self.module, self.sources = kind, module, tuple(sources)


class Registry:
    """versions: name -> versions newest first; manifests: (name, version) -> [dict(name, range, exact, scope)]."""

    def __init__(self, versions, manifests):
        self.versions, self.manifests = versions, manifests

    def search(self, name, expr):
        out = [v for v in self.versions.get(name, []) if v in expr]
        return out[::-1] if "oldest-first" in MUT else out


def has_scope(phases, scope):
    if scope is None or "scoped-always" in MUT:
        return True
    return scope in phases


def build_dependency_graph(root, root_ver, reg: Registry, installed, installed_reqs, known, phases=(), ignore_installed=False,
                           permit_downgrade=False, check_nested_scoped=False, refresh=(), universe=None):
    """Returns (graph, sources) or raises ResolveError. installed: name -> version; installed_reqs: [(by, on, set)]."""
    graph, sources, work, cache = {}, {}, {}, {}
    work[(1, root)] = ((root,), None)
    cache[root] = root_ver
    q_phases = phases if check_nested_scoped else ()
    all_versions = universe

    def manifest(name):
        return reg.manifests.get((name, cache[name]), [])

    def AND(expr, other, dep, constraints):
        out = expr & other
        if not out:
            raise ResolveError("clash", dep, constraints)
        return out

    def process(exact, dep, dep_path, depth, parent_info):
        name = dep["name"]
        expr = frozenset(dep["range"])
        constraints = [(parent_info, "req")]
        for (dname, src), other in sorted(sources.items()):
            if dname == name:
                constraints.append((src, "req"))
                expr = AND(expr, other, name, constraints)
        if not ignore_installed:
            other_deps = list(known) + list(graph)
            for (by, on, rng) in installed_reqs:
                if on == name and by not in other_deps:
                    constraints.append((by, "installed"))
                    expr = AND(expr, frozenset(rng), name, constraints)
        prev_version = graph[name] if name in graph else None
        if not ignore_installed and name in installed:
            iv = installed[name]
            valid = iv in expr and (prev_version is None or prev_version == iv)
            if valid and name not in refresh:
                graph[name] = iv
                sources[(name, parent_info)] = frozenset(dep["range"])
                work[(depth + 1, name)] = (dep_path + (name,), dep["scope"])
                cache[name] = iv
                return
            if not permit_downgrade and "no-floor" not in MUT:
                floor = frozenset(v for v in all_versions if v >= iv)
                constraints.append((name, "installed-floor"))
                expr = AND(expr, floor, name, constraints)
        refs = reg.search(name, expr)
        if not refs:
            raise ResolveError("notfound", name, constraints)
        if exact:
            v = refs[0]
            graph[name] = v
            sources[(name, parent_info)] = frozenset(dep["range"])
            work[(depth + 1, name)] = (dep_path + (name,), dep["scope"])
            cache[name] = v
            return
        chosen = None
        for v in refs:
            trial = dict(graph)
            trial[name] = v
            ok = True
            for sub in reg.manifests.get((name, v), []):
                if not has_scope(q_phases, sub["scope"]):
                    continue
                if sub["name"] in trial and trial[sub["name"]] not in sub["range"]:
                    ok = False
                    break
            chosen = v
            if ok:
                break
        graph[name] = chosen
        sources[(name, parent_info)] = frozenset(dep["range"])
        work[(depth + 1, name)] = (dep_path + (name,), dep["scope"])
        cache[name] = chosen

    depth = 1
    while any(d == depth for (d, _n) in work):
        done_names = set()
        while True:
            pending = sorted(n for (d, n) in work if d == depth and n not in done_names)
            if not pending:
                break
            mod = pending[0]
            done_names.add(mod)
            path, scope = work[(depth, mod)]
            if scope is not None and len(path) == 1:
                path = ()
            exact_deps, fuzzy_deps = [], []
            for i, dep in enumerate(manifest(mod)):
                dep_path = path
                if not has_scope(phases, dep["scope"]):
                    continue
                if dep["scope"] is not None and len(path) <= 1:
                    dep_path = ()
                if dep["name"] in dep_path:
                    raise ResolveError("cycle", mod, dep_path + (dep["name"],))
                (exact_deps if dep["exact"] else fuzzy_deps).append((dep, dep_path))
            parent_info = (mod, cache[mod])
            for dep, dep_path in exact_deps:
                process(True, dep, dep_path, depth, parent_info)
            for dep, dep_path in fuzzy_deps:
                process(False, dep, dep_path, depth, parent_info)
        for key in [k for k in work if k[0] == depth]:
            del work[key]
        depth += 1
        if depth > 200:
            raise ResolveError("bound", root)
    graph.pop(root, None)
    for key in [k for k in sources if k[1][0] == root]:
        del sources[key]
    return graph, sources


def flat_install_order(graph, sources):
    """ConstructInvertedDependencyGraph + GetFlatDependencyListFromInvertedDependencyGraph (Utils/Module.cls)."""
    inv = {name: set() for name in graph}
    for (name, src) in sources:
        if src[0] in inv:
            inv[src[0]].add(name)
    order = []
    while inv:
        progressed = False
        for name in sorted(inv):
            if name in inv and not inv[name]:
                order.append(name)
                del inv[name]
                for other in inv:
                    inv[other].discard(name)
                progressed = True
        if not progressed:
            raise ResolveError("cycle-in-flat-list", sorted(inv)[0])
    return order
