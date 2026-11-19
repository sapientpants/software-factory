#!/usr/bin/env python3
"""PROTOTYPE, throwaway. Answers probe P-7 (SPEC 9.4, WS-9, issue #6).

Builds a mirror, a host.git and a workspace as WS-1 and WS-10 say, makes the
workspace hostile, then runs the capture of SPEC 9.4 as four layers and prints
the verdict of every layer, also past the first rejection:

  1 scan     host, lstat only
  2 listing  sandboxed git lists the change set; the host checks it against
             the scan's inventory
  3 stage    sandboxed git stages the listed paths, writes a tree and a pack
  4 ingest   host indexes the pack into host.git and walks the tree

Then commit and export. The sandbox is left out unless SF_PROTO_BWRAP=1.
Nothing here is production code: no error handling, no tests, no polish.

  python3 capture_proto.py            run everything, print a result per case
  python3 capture_proto.py -v         also print every git command
  python3 capture_proto.py -k fifo    only cases whose name contains "fifo"
"""

import os
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import unicodedata

GIT = shutil.which("git")
EMPTY_TREE = "4b825dc642cb6eb9a060e54bf8d69288fbee4904"
FILE_CAP = 5 * 2**20
TOTAL_CAP = 50 * 2**20
PACK_CAP = TOTAL_CAP + 2**20
VERBOSE = "-v" in sys.argv
BWRAP = os.environ.get("SF_PROTO_BWRAP") == "1"
LINUX = sys.platform.startswith("linux")

# The environment of every git process the host starts, sandboxed or not.
BASE_ENV = {
    "PATH": os.path.dirname(GIT) + ":/usr/bin:/bin",
    "HOME": "/nonexistent",
    "LC_ALL": "C",
    "GIT_CONFIG_NOSYSTEM": "1",
    "GIT_CONFIG_GLOBAL": "/dev/null",
    "GIT_ATTR_NOSYSTEM": "1",
    "GIT_TERMINAL_PROMPT": "0",
    "GIT_OPTIONAL_LOCKS": "0",
    "GIT_NO_REPLACE_OBJECTS": "1",
    "GIT_NO_LAZY_FETCH": "1",
}
IDENT = {
    "GIT_AUTHOR_NAME": "sf-factory[bot]",
    "GIT_AUTHOR_EMAIL": "1+sf-factory[bot]@users.noreply.github.com",
    "GIT_AUTHOR_DATE": "2026-11-19T12:00:00Z",
    "GIT_COMMITTER_NAME": "sf-factory[bot]",
    "GIT_COMMITTER_EMAIL": "1+sf-factory[bot]@users.noreply.github.com",
    "GIT_COMMITTER_DATE": "2026-11-19T12:00:00Z",
}

# quarantine/config, written by the host. The staging git reads no other config.
STAGE_CONFIG = """[core]
	repositoryformatversion = 0
	bare = false
	fileMode = true
	symlinks = true
	ignoreCase = false
	precomposeUnicode = false
	protectHFS = true
	protectNTFS = true
	autocrlf = false
	safecrlf = false
	fsmonitor = false
	untrackedCache = false
	hooksPath = /dev/null
	attributesFile = /dev/null
	excludesFile = /dev/null
	logAllRefUpdates = false
[gc]
	auto = 0
[maintenance]
	auto = false
[submodule]
	recurse = false
"""


class Out:
    def __init__(self, rc, out, err):
        self.rc, self.out, self.err = rc, out, err

    @property
    def text(self):
        return self.out.decode("utf-8", "surrogateescape").strip()

    @property
    def errtext(self):
        return self.err.decode("utf-8", "replace").strip()


def sh(argv, env=None, stdin=None, cwd=None, timeout=60, show=None):
    if VERBOSE and show is not False:
        extra = {k: v for k, v in (env or {}).items() if BASE_ENV.get(k) != v and k not in IDENT}
        pre = " ".join(f"{k}={shlex.quote(v)}" for k, v in extra.items())
        print("      $", pre, " ".join(shlex.quote(str(a)) for a in argv))
    try:
        p = subprocess.run(
            [str(a) for a in argv], env=env, input=stdin, cwd=cwd, capture_output=True, timeout=timeout
        )
        return Out(p.returncode, p.stdout, p.stderr)
    except subprocess.TimeoutExpired:
        return Out(-999, b"", f"TIMEOUT after {timeout}s".encode())


def env(extra=None):
    e = dict(BASE_ENV)
    e.update(extra or {})
    return e


def no_attrs(attr):
    """In-tree .gitattributes are never applied: the attribute source is the empty tree.
    As a flag, because a git older than 2.40 rejects the flag but would ignore GIT_ATTR_SOURCE."""
    return ["--attr-source=" + EMPTY_TREE] if attr else []


def git(args, cwd=None, extra=None, stdin=None, attr=True, timeout=60, show=None):
    return sh([GIT] + no_attrs(attr) + args, env=env(extra), stdin=stdin, cwd=cwd, timeout=timeout, show=show)


def must(o):
    if o.rc != 0:
        raise RuntimeError(f"rc={o.rc}: {o.errtext}")
    return o


def w(path, data, mode=None):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as f:
        f.write(data if isinstance(data, bytes) else data.encode())
    if mode is not None:
        os.chmod(path, mode)


# --------------------------------------------------------------------------
# Fixture: upstream, mirror, an unmerged branch of another run, things outside
# --------------------------------------------------------------------------


class Fixture:
    def __init__(self, root, base_files=None, name="mirror"):
        self.root = root
        self.outside = os.path.join(root, "outside")
        self.canary = os.path.join(self.outside, "CANARY-FIRED")
        self.canary_sh = os.path.join(self.outside, "canary.sh")
        if not os.path.exists(self.canary_sh):
            w(self.canary_sh, f"#!/bin/sh\necho fired >> {shlex.quote(self.canary)}\n", 0o755)
            w(os.path.join(self.outside, "dir/lib.rs"), "OUTSIDE CONTENT\n")
            w(os.path.join(self.outside, "ignore-all"), "*\n")
        up = os.path.join(root, name + "-upstream")
        self.mirror = os.path.join(root, name + ".git")
        files = base_files or {
            "README.md": "# fixture\n",
            "src/lib.rs": "pub fn f() {}\n",
            "src/main.rs": "fn main() {}\n",
            "scripts/run.sh": ("#!/bin/sh\n", 0o755),
            ".gitignore": "target/\n*.log\n",
            "build/keep.log": "tracked although *.log is ignored\n",
            "docs/guide.md": "guide\n",
        }
        g = lambda a, **k: must(git(a, cwd=up, extra=IDENT, show=False, **k))
        os.makedirs(up)
        g(["init", "-q", "-b", "main", "--template=", "."])
        for p, v in files.items():
            data, mode = v if isinstance(v, tuple) else (v, 0o644)
            w(os.path.join(up, p), data, mode)
        g(["add", "-A", "-f", "."], attr=("attrs" in name))
        g(["commit", "-q", "-m", "base"])
        self.base = g(["rev-parse", "HEAD"]).text
        g(["checkout", "-q", "-b", "factory/issue-99"])
        w(os.path.join(up, "other-run.txt"), "content of another run's unmerged branch\n")
        g(["add", "other-run.txt"])
        g(["commit", "-q", "-m", "other run"])
        self.other_blob = g(["rev-parse", "HEAD:other-run.txt"]).text
        self.other_commit = g(["rev-parse", "HEAD"]).text
        g(["checkout", "-q", "main"])
        must(git(["init", "-q", "--bare", "--template=", self.mirror], show=False))
        g(["push", "-q", self.mirror, "main", "factory/issue-99"])

    def fired(self):
        return os.path.exists(self.canary)


class Run:
    """runs/<id>/ of SPEC 9.1."""

    def __init__(self, fx, name, attr=True, show=False):
        self.fx = fx
        self.dir = os.path.join(fx.root, "runs", re.sub(r"[^a-z0-9]+", "-", name.lower()))
        self.ws = os.path.join(self.dir, "workspace")
        self.host = os.path.join(self.dir, "host.git")
        self.q = os.path.join(self.dir, "quarantine")
        os.makedirs(self.dir)
        # WS-10
        must(git(["clone", "-q", "--bare", "--no-local", "--single-branch", "--branch", "main",
                  "--template=", fx.mirror, self.host], show=show))
        must(git(["remote", "remove", "origin"], cwd=self.host, show=show))
        # WS-1
        must(git(["clone", "-q", "--no-local", "--no-checkout", "--single-branch", "--branch", "main",
                  "--template=", fx.mirror, self.ws], show=show))
        must(git(["checkout", "-q", "--detach", fx.base], cwd=self.ws, attr=attr, show=show))
        must(git(["remote", "remove", "origin"], cwd=self.ws, show=show))
        self.parent = fx.base
        self.prev_tree = must(self.hgit(["rev-parse", fx.base + "^{tree}"], show=False)).text

    def p(self, rel):
        return os.path.join(self.ws, rel)

    def hgit(self, args, **k):
        """Host-side git on bare host.git. Never sandboxed, never sees the workspace."""
        return git(["--git-dir=" + self.host] + args, **k)

    def sgit(self, args, attr=True, stdin=None, timeout=60):
        """Staging git: the only git that sees the workspace. Sandboxed in the real design."""
        e = env({"GIT_INDEX_FILE": os.path.join(self.q, "index")})
        argv = [GIT] + no_attrs(attr) + ["--git-dir=" + self.q, "--work-tree=" + self.ws] + args
        if BWRAP:
            b = ["bwrap", "--unshare-all", "--die-with-parent", "--new-session", "--clearenv",
                 "--ro-bind", "/usr", "/usr", "--symlink", "usr/bin", "/bin", "--symlink", "usr/lib", "/lib"]
            if os.path.exists("/lib64"):
                b += ["--symlink", "usr/lib64", "/lib64"]
            b += ["--proc", "/proc", "--dev", "/dev",
                  "--ro-bind", self.ws, self.ws,
                  "--bind", self.q, self.q,
                  "--ro-bind", os.path.join(self.q, "config"), os.path.join(self.q, "config"),  # SEC-6
                  "--ro-bind", os.path.join(self.host, "objects"), os.path.join(self.host, "objects"),
                  "--chdir", self.q]
            for k, v in e.items():
                b += ["--setenv", k, v]
            argv = b + ["--"] + argv
            if VERBOSE:
                print("      $ [bwrap: workspace ro, quarantine rw but its config ro, host.git/objects ro, no network]")
        return sh(argv, env=e, stdin=stdin, cwd=self.q if not BWRAP else None, timeout=timeout)


# --------------------------------------------------------------------------
# Layer 1: the scan. lstat only, never follows a symlink, opens directories only.
# --------------------------------------------------------------------------

HFS_IGNORABLE = dict.fromkeys(
    list(range(0x200C, 0x2010)) + list(range(0x202A, 0x202F)) + list(range(0x206A, 0x2070)) + [0xFEFF]
)


def is_dotgit(name):
    """Approximates git's is_hfs_dotgit and is_ntfs_dotgit. The real scan must port both."""
    for piece in name.split("\\"):
        if piece.translate(HFS_IGNORABLE).casefold() == ".git":
            return True
        ntfs = piece.split(":")[0].rstrip(" .").lower()
        if ntfs in (".git", "git~1"):
            return True
    return False


def scan(ws):
    """Returns (inventory, errors). inventory: path -> (kind, size, mode, nlink)."""
    inv, errs = {}, []

    def walk(d, rel):
        try:
            entries = sorted(os.scandir(d), key=lambda e: e.name)
        except OSError as e:
            errs.append(f"{rel or '.'}: cannot list ({e.strerror})")
            return
        for e in entries:
            path = rel + e.name
            st = e.stat(follow_symlinks=False)
            if rel == "" and e.name == ".git":
                continue  # the root .git: never read, never descended into
            if is_dotgit(e.name):
                errs.append(f"{path!r}: name equivalent to .git")
            if os.path.stat.S_ISREG(st.st_mode):
                inv[path] = ("f", st.st_size, st.st_mode & 0o7777, st.st_nlink)
            elif os.path.stat.S_ISLNK(st.st_mode):
                inv[path] = ("l", st.st_size, 0, st.st_nlink)
            elif os.path.stat.S_ISDIR(st.st_mode):
                inv[path] = ("d", 0, st.st_mode & 0o7777, st.st_nlink)
                walk(os.path.join(d, e.name), path + "/")
            else:
                errs.append(f"{path!r}: not a regular file, directory or symlink")

    walk(ws, "")
    return inv, errs


# --------------------------------------------------------------------------
# The capture, layer by layer
# --------------------------------------------------------------------------


class Result:
    def __init__(self):
        self.scan = []      # layer 1 rejections
        self.listing = []   # entries of the change set: (status, path)
        self.listrej = []   # layer 2 rejections
        self.stage = []     # layer 3 errors from git
        self.ingest = []    # layer 4 rejections
        self.tree = self.commit = self.export = None
        self.notes = []

    def first(self):
        for name in ("scan", "listrej", "stage", "ingest"):
            if getattr(self, name):
                return {"listrej": "listing"}.get(name, name)
        return "candidate"


def prepare_quarantine(r):
    shutil.rmtree(r.q, ignore_errors=True)
    must(git(["init", "-q", "--bare", "--template=", r.q]))
    w(os.path.join(r.q, "config"), STAGE_CONFIG)
    w(os.path.join(r.q, "objects/info/alternates"), os.path.join(r.host, "objects") + "\n")


def list_changes(r, attr=True, timeout=60, submodules="all", fresh=True):
    """Sandboxed. Fresh index from the previous tree, then the change set with ignore rules."""
    errs = []
    if fresh:
        o = r.sgit(["read-tree", r.prev_tree], attr=attr)
        if o.rc:
            errs.append("read-tree: " + o.errtext)
        r.sgit(["update-index", "-q", "--refresh"], attr=attr, timeout=timeout)
    listing = []
    # Tracked paths that changed: M modified (content or mode), D deleted, T type changed.
    o = r.sgit(["diff-files", "-z", "--raw", "--no-renames", "--ignore-submodules=" + submodules],
               attr=attr, timeout=timeout)
    if o.rc:
        errs.append("diff-files: " + o.errtext)
    f = o.out.split(b"\0")
    for meta, path in zip(f[0::2], f[1::2]):
        listing.append((meta.split()[4].decode(), os.fsdecode(path)))
    # Untracked paths, minus the ignored ones. This is the ignore-rule mechanism.
    u = r.sgit(["ls-files", "-z", "--others", "--exclude-standard"], attr=attr, timeout=timeout)
    if u.rc:
        errs.append("ls-files: " + u.errtext)
    listing += [("?", os.fsdecode(p)) for p in u.out.split(b"\0") if p]
    return listing, errs, (o.errtext + " " + u.errtext).strip()


def check_listing(listing, inv):
    """Host. Every path git wants to stage must be a file or symlink the scan saw, within the caps."""
    rej, total = [], 0
    for st, path in listing:
        if st == "D":
            continue
        kind, size = (inv.get(path) or ("-", 0))[:2]
        if path.endswith("/"):
            rej.append(f"{path!r}: a directory git will not descend into (nested repository)")
        elif kind not in ("f", "l"):
            rej.append(f"{path!r}: not a file or symlink in the scan inventory (kind {kind})")
        elif size > FILE_CAP:
            rej.append(f"{path!r}: {size} bytes, over the per-file cap")
        total += size
    if total > TOTAL_CAP:
        rej.append(f"change set of {total} bytes, over the total cap")
    return rej


def stage(r, listing, attr=True):
    """Sandboxed. Stage exactly the listed paths, write the tree, emit the new objects as a pack."""
    errs = []
    nul = lambda want: b"".join(os.fsencode(p) + b"\0" for st, p in listing if (st == "D") == want)
    # Deletions first: a deleted path may now lie behind a symlink or under a file of the same name.
    for args, paths in ((["--force-remove"], nul(True)), (["--add"], nul(False))):
        o = r.sgit(["update-index", "-z"] + args + ["--stdin"], stdin=paths, attr=attr)
        if o.rc or o.err:
            errs.append(f"update-index {args[0]} (exit {o.rc}): " + o.errtext.replace("\n", " | ")[:300])
    # update-index skips a path it will not take with a warning and exit 0. So ask again: what still differs?
    left, _, _ = list_changes(r, attr, fresh=False)
    if left:
        errs.append("not staged: " + ", ".join(repr(p) for _, p in left[:3]) + (" …" if len(left) > 3 else ""))
    o = r.sgit(["write-tree"], attr=attr)
    if o.rc:
        return None, b"", errs + ["write-tree: " + o.errtext]
    tree = o.text
    o = r.sgit(["pack-objects", "--revs", "--stdout", "--local", "-q"], stdin=tree.encode() + b"\n")
    if o.rc:
        errs.append("pack-objects: " + o.errtext)
    return tree, o.out, errs  # the sandbox returns two things: a tree name and a pack on stdout


def tree_entries(r, tree, show=None, extra=None):
    # Not --format: with it git C-quotes %(path) even under -z.
    o = must(r.hgit(["ls-tree", "-r", "-t", "-z", "-l", "--full-tree", tree], show=show, extra=extra))
    ents = {}
    for e in o.out.split(b"\0"):
        if e:
            meta, path = e.split(b"\t", 1)
            mode, typ, oid, size = meta.decode().split()
            ents[os.fsdecode(path)] = (mode, typ, 0 if size == "-" else int(size), oid)
    return ents


def ingest(r, tree, pack):
    """Host, bare host.git. Everything the sandbox returned is untrusted: a pack and a tree name.
    The pack is indexed into a host-side object directory of its own, as git receive-pack does,
    and moves into host.git only when every check has passed."""
    rej = _ingest(r, tree, pack)
    if not rej:
        src, dst = os.path.join(r.incoming, "pack"), os.path.join(r.host, "objects/pack")
        for f in sorted(os.listdir(src), key=lambda f: f.endswith(".idx")):  # the .idx last: it publishes the pack
            os.rename(os.path.join(src, f), os.path.join(dst, f))
    shutil.rmtree(r.incoming)
    return rej


def _ingest(r, tree, pack):
    rej = []
    r.incoming = os.path.join(r.dir, "incoming")
    shutil.rmtree(r.incoming, ignore_errors=True)
    os.makedirs(os.path.join(r.incoming, "pack"))
    if not re.fullmatch(r"[0-9a-f]{40}", tree or ""):
        return [f"tree name {tree!r} is not an object name"]
    r.incoming_env = {"GIT_OBJECT_DIRECTORY": r.incoming,
                      "GIT_ALTERNATE_OBJECT_DIRECTORIES": os.path.join(r.host, "objects")}
    o = r.hgit(["index-pack", "--stdin", "--strict", f"--max-input-size={PACK_CAP}"], stdin=pack,
               extra=r.incoming_env)
    if o.rc:
        return ["index-pack --strict: " + o.errtext.replace("\n", " | ")]
    if r.hgit(["cat-file", "-t", tree], extra=r.incoming_env).text != "tree":
        return [f"{tree} is not a tree"]
    ents = tree_entries(r, tree, extra=r.incoming_env)
    seen = {}
    for path, (mode, typ, size, _) in ents.items():
        if mode not in ("040000", "100644", "100755", "120000"):
            rej.append(f"{path!r}: mode {mode}")
        key = unicodedata.normalize("NFC", unicodedata.normalize("NFD", path).casefold())
        if key in seen:
            rej.append(f"{path!r} collides with {seen[key]!r} after case folding and normalisation")
        seen[key] = path
    o = must(r.hgit(["diff-tree", "-r", "-z", "--raw", "--no-renames", "--no-abbrev", r.prev_tree, tree],
                    extra=r.incoming_env))
    f = o.out.split(b"\0")
    total = 0
    for meta, path in zip(f[0::2], f[1::2]):
        if meta.split()[4] != b"D":
            size = ents[os.fsdecode(path)][2]
            total += size
            if size > FILE_CAP:
                rej.append(f"{os.fsdecode(path)!r}: blob of {size} bytes, over the per-file cap")
    if total > TOTAL_CAP:
        rej.append(f"change set of {total} bytes, over the total cap")
    return rej


def commit(r, tree, subject="fix: prototype candidate"):
    msg = f"{subject}\n\nFactory-Issue: 6\nFactory-Run: {os.path.basename(r.dir)}\n"
    return must(r.hgit(["commit-tree", "--no-gpg-sign", tree, "-p", r.parent, "-F", "-"],
                       extra=IDENT, stdin=msg.encode())).text


def export(r, sha, attr=True, name=None):
    """Host. A new, empty, host-created directory; a temporary index; checkout-index."""
    dest = os.path.join(r.dir, "export", name or sha)
    os.makedirs(dest)
    idx = {"GIT_INDEX_FILE": os.path.join(r.dir, "export", (name or sha) + ".index")}
    must(r.hgit(["read-tree", sha], extra=idx, attr=attr))
    o = git(["--git-dir=" + r.host, "--work-tree=" + dest, "checkout-index", "-a"], extra=idx, attr=attr)
    return dest, o


def same_as_workspace(r, tree, dest):
    """Test oracle only (it reads the workspace): is every exported path byte-identical to it?"""
    diffs = []
    for path, (mode, typ, _, _) in tree_entries(r, tree, show=False).items():
        if typ != "blob":
            continue
        a, b = os.path.join(r.ws, path), os.path.join(dest, path)
        if mode == "120000":
            if not os.path.islink(b) or os.readlink(a) != os.readlink(b):
                diffs.append(path)
        elif open(a, "rb").read() != open(b, "rb").read() or os.access(a, os.X_OK) != os.access(b, os.X_OK):
            diffs.append(path)
    return diffs


def capture(r, attr=True, timeout=60, export_attr=None):
    """Runs every layer and records every verdict, also past the first rejection."""
    res = Result()
    inv, res.scan = scan(r.ws)
    prepare_quarantine(r)
    res.listing, errs, res.status_stderr = list_changes(r, attr, timeout)
    res.stage += errs
    res.listrej = check_listing(res.listing, inv)
    # Past a layer 2 rejection the prototype stages anyway, to see what layers 3 and 4 do. Not a terabyte, though.
    forced = [(s, p) for s, p in res.listing if (inv.get(p) or ("-", 0))[1] < 64 * 2**20]
    res.tree, pack, errs = stage(r, forced, attr)
    res.stage += errs
    res.ingest = ingest(r, res.tree, pack)
    if res.first() == "candidate":
        res.commit = commit(r, res.tree)
        res.export, o = export(r, res.commit, attr if export_attr is None else export_attr)
        if o.rc:
            res.notes.append("export failed: " + o.errtext)
        d = same_as_workspace(r, res.tree, res.export)
        res.notes.append("export differs from workspace at " + repr(d) if d else "export == workspace")
    return res


# --------------------------------------------------------------------------
# Cases. Each makes a fresh workspace hostile and names the layer that must stop it.
# --------------------------------------------------------------------------

CASES = []


def case(name, expect, linux_only=False):
    def deco(fn):
        CASES.append((name, expect, fn, linux_only))
        return fn
    return deco


def hostile_config(fx):
    c = fx.canary_sh
    return (f"[core]\n\tfsmonitor = {c}\n\thooksPath = {fx.outside}/hooks\n\tpager = {c}\n"
            f"\tattributesFile = {fx.outside}/attrs\n\texcludesFile = {fx.outside}/ignore-all\n"
            f"\tsshCommand = {c}\n\teditor = {c}\n"
            f"[filter \"evil\"]\n\tclean = {c}\n\tsmudge = {c}\n\tprocess = {c}\n\trequired = true\n"
            f"[diff]\n\texternal = {c}\n[alias]\n\tstatus = !{c}\n\tread-tree = !{c}\n")


def nested_repo(fx, path, commits=True):
    os.makedirs(path)
    g = lambda a: must(git(a, cwd=path, extra=IDENT, show=False))
    g(["init", "-q", "-b", "main", "--template=", "."])
    w(os.path.join(path, "inner.txt"), "inner\n")
    if commits:
        g(["add", "inner.txt"])
        g(["commit", "-q", "-m", "inner"])
    with open(os.path.join(path, ".git/config"), "a") as f:
        f.write(hostile_config(fx))
    for h in ("post-index-change", "pre-commit", "reference-transaction", "post-checkout", "fsmonitor-watchman"):
        w(os.path.join(path, ".git/hooks", h), f"#!/bin/sh\nexec {fx.canary_sh}\n", 0o755)


def has(res, r, path):
    return path in tree_entries(r, res.tree)


@case("benign change set", "candidate")
def _(r):
    w(r.p("src/lib.rs"), "pub fn changed() {}\n")
    w(r.p("src/new.rs"), "pub fn new() {}\n")
    os.remove(r.p("docs/guide.md"))
    os.symlink("lib.rs", r.p("src/link.rs"))
    w(r.p("target/debug/app"), "ignored build output\n")
    w(r.p("build/keep.log"), "tracked, matches *.log, modified\n")
    w(r.p("build/new.log"), "untracked, matches *.log\n")
    os.makedirs(r.p("emptydir"))
    os.remove(r.p("README.md"))                 # a tracked file becomes a directory
    w(r.p("README.md/index.md"), "# now a directory\n")
    shutil.rmtree(r.p("scripts"))               # a tracked directory becomes a file
    w(r.p("scripts"), "now a file\n")

    def check(res):
        t = tree_entries(r, res.tree)
        want = {"src/new.rs": True, "docs/guide.md": False, "src/link.rs": True, "target": False,
                "build/new.log": False, "emptydir": False, "README.md/index.md": True, "scripts/run.sh": False}
        bad = [p for p, v in want.items() if (p in t) != v]
        if t["src/link.rs"][0] != "120000":
            bad.append("link mode")
        keep = must(r.hgit(["cat-file", "blob", res.tree + ":build/keep.log"])).out
        if b"modified" not in keep:
            bad.append("tracked file under an ignore rule lost its change")
        return bad
    return check


@case("executable bits", "candidate")
def _(r):
    os.chmod(r.p("README.md"), 0o755)           # mode-only changes, both directions
    os.chmod(r.p("scripts/run.sh"), 0o644)
    w(r.p("scripts/new.sh"), "#!/bin/sh\n", 0o755)
    w(r.p("scripts/setuid.sh"), "#!/bin/sh\n", 0o4755)
    w(r.p("scripts/world.sh"), "#!/bin/sh\n", 0o666)

    def check(res):
        t = tree_entries(r, res.tree)
        got = {p: t[p][0] for p in ("README.md", "scripts/run.sh", "scripts/new.sh", "scripts/setuid.sh",
                                    "scripts/world.sh")}
        res.notes.append(f"modes {got}")
        want = ["100755", "100644", "100755", "100755", "100644"]
        return [] if list(got.values()) == want else ["modes"]
    return check


@case("unreadable file", "stage")
def _(r):
    w(r.p("src/locked.rs"), "cannot read\n", 0o000)


@case("root .git made hostile (config, hooks, info/exclude, info/attributes)", "candidate")
def _(r):
    with open(r.p(".git/config"), "a") as f:
        f.write(hostile_config(r.fx))
    for h in ("post-index-change", "pre-commit", "reference-transaction", "post-checkout"):
        w(r.p(".git/hooks/" + h), f"#!/bin/sh\nexec {r.fx.canary_sh}\n", 0o755)
    w(r.p(".git/info/exclude"), "*\n")
    w(r.p(".git/info/attributes"), "* filter=evil\n")
    os.mkfifo(r.p(".git/fifo"))
    w(r.p("src/new.rs"), "pub fn new() {}\n")
    return lambda res: [] if has(res, r, "src/new.rs") else ["workspace info/exclude was honoured"]


@case("root .git replaced by a gitfile", "candidate")
def _(r):
    nested_repo(r.fx, os.path.join(r.fx.outside, "elsewhere-" + os.path.basename(r.dir)))
    shutil.rmtree(r.p(".git"))
    w(r.p(".git"), f"gitdir: {r.fx.outside}/elsewhere-{os.path.basename(r.dir)}/.git\n")
    w(r.p("src/new.rs"), "pub fn new() {}\n")
    return lambda res: [] if has(res, r, "src/new.rs") else ["lost src/new.rs"]


@case("nested repository (directory, with commits, fsmonitor, hooks)", "scan")
def _(r):
    nested_repo(r.fx, r.p("vendor/nested"))


@case("nested repository (directory, no commits)", "scan")
def _(r):
    nested_repo(r.fx, r.p("vendor/nested"), commits=False)


@case("nested repository (gitfile)", "scan")
def _(r):
    nested_repo(r.fx, os.path.join(r.fx.outside, "gf-" + os.path.basename(r.dir)))
    w(r.p("vendor/nested/.git"), f"gitdir: {r.fx.outside}/gf-{os.path.basename(r.dir)}/.git\n")
    w(r.p("vendor/nested/inner.txt"), "inner\n")


@case("nested repository (.git is a symlink to a repository)", "scan")
def _(r):
    nested_repo(r.fx, os.path.join(r.fx.outside, "sl-" + os.path.basename(r.dir)))
    os.makedirs(r.p("vendor/nested"))
    os.symlink(f"{r.fx.outside}/sl-{os.path.basename(r.dir)}/.git", r.p("vendor/nested/.git"))
    w(r.p("vendor/nested/inner.txt"), "inner\n")


@case(".git file that names no repository", "scan")
def _(r):
    w(r.p("vendor/odd/.git"), "gitdir: /nonexistent\n")
    w(r.p("vendor/odd/inner.txt"), "inner\n")
    r.note = "without the scan: git descends, stages inner.txt and silently leaves .git out"


@case("nested repository in an ignored directory", "scan")
def _(r):
    nested_repo(r.fx, r.p("target/nested"))


for label, dirname in ((".GIT", ".GIT"), (".git with trailing space (NTFS)", ".git "), ("git~1 (NTFS short name)", "git~1"),
                       (".g<U+200C>it (HFS)", ".g‌it")):
    @case(f"nested repository ({label})", "scan", linux_only=(dirname == ".GIT"))
    def _(r, dirname=dirname):
        nested_repo(r.fx, r.p("vendor/tmp"))
        os.rename(r.p("vendor/tmp/.git"), r.p("vendor/tmp/" + dirname))


@case("file named with backslashes around .git", "scan")
def _(r):
    w(r.p("vendor/x\\.git\\config"), "[core]\n")


@case("embedded bare repository (no .git name anywhere)", "candidate")
def _(r):
    nested_repo(r.fx, r.p("vendor/tmp"))
    os.rename(r.p("vendor/tmp/.git"), r.p("vendor/bare"))
    shutil.rmtree(r.p("vendor/tmp"))

    def check(res):
        t = tree_entries(r, res.tree)
        res.notes.append("tree holds " + ", ".join(p for p in t if p.startswith("vendor/bare/") and "/" not in p[12:]))
        return []
    return check


@case("symlinked directories", "candidate")
def _(r):
    os.symlink("/etc", r.p("etc-link"))
    os.symlink(r.fx.outside + "/dir", r.p("outside-link"))
    shutil.rmtree(r.p("src"))
    os.symlink(r.fx.outside + "/dir", r.p("src"))  # a tracked directory becomes a symlink; lib.rs exists behind it
    os.symlink("../docs", r.p("scripts/docs-link"))
    os.symlink("nowhere", r.p("dangling"))

    def check(res):
        t = tree_entries(r, res.tree)
        bad = [p for p in ("etc-link", "outside-link", "src", "scripts/docs-link", "dangling") if t.get(p, ("",))[0] != "120000"]
        bad += [p for p in t if p.startswith(("src/", "etc-link/", "outside-link/"))]
        blobs = must(r.hgit(["cat-file", "--batch-all-objects", "--batch"])).out
        if b"OUTSIDE CONTENT" in blobs:
            bad.append("content from behind a symlink entered host.git")
        return bad
    return check


@case("FIFO at the top level", "scan")
def _(r):
    os.mkfifo(r.p("pipe"))


@case("FIFO in an ignored directory", "scan")
def _(r):
    os.makedirs(r.p("target"))
    os.mkfifo(r.p("target/pipe"))


@case("tracked file replaced by a FIFO", "scan")
def _(r):
    os.remove(r.p("src/lib.rs"))
    os.mkfifo(r.p("src/lib.rs"))


@case("unix socket", "scan")
def _(r):
    import socket
    s = socket.socket(socket.AF_UNIX)
    cwd = os.getcwd()
    os.chdir(r.ws)
    s.bind("sock")
    os.chdir(cwd)
    s.close()


@case("hard links (inside the workspace and to a file outside it)", "candidate")
def _(r):
    w(r.p("src/a.rs"), "linked\n")
    os.link(r.p("src/a.rs"), r.p("src/b.rs"))
    os.link(r.fx.outside + "/dir/lib.rs", r.p("src/outside.rs"))

    def check(res):
        t = tree_entries(r, res.tree)
        res.notes.append("two entries, one blob" if t["src/a.rs"][3] == t["src/b.rs"][3] else "?")
        st = os.lstat(os.path.join(res.export, "src/b.rs"))
        return [] if st.st_nlink == 1 and "src/outside.rs" in t else ["export kept the hard link"]
    return check


@case(".gitattributes: filter, eol, encoding, ident, export-ignore, -diff", "candidate")
def _(r):
    w(r.p(".gitattributes"),
      "*.txt filter=evil\ncrlf.txt text eol=crlf\nutf16.txt working-tree-encoding=UTF-16\n"
      "id.txt ident\nsecret.txt export-ignore\n*.rs -diff\n*.bin filter=lfs diff=lfs merge=lfs -text\n")
    with open(r.p(".git/config"), "a") as f:
        f.write(hostile_config(r.fx))
    w(r.p("crlf.txt"), b"one\r\ntwo\r\n")
    w(r.p("utf16.txt"), "héllo\n".encode("utf-16"))
    w(r.p("id.txt"), "$Id: 0123456789012345678901234567890123456789 $\n")
    w(r.p("secret.txt"), "must reach the export\n")
    w(r.p("src/lib.rs"), "pub fn changed() {}\n")
    w(r.p("asset.bin"), "version https://git-lfs.github.com/spec/v1\noid sha256:" + "0" * 64 + "\nsize 12345\n")

    def check(res):
        bad = [] if os.path.exists(os.path.join(res.export, "secret.txt")) else ["export-ignore honoured by the export"]
        o = r.hgit(["archive", "--format=tar", res.commit], attr=False)
        names = sh(["tar", "-t"], stdin=o.out, show=False).text.split()
        res.notes.append("git archive omits secret.txt" if "secret.txt" not in names else "git archive keeps secret.txt")
        a = r.hgit(["diff-tree", "-p", "--no-ext-diff", "--no-textconv", r.fx.base, res.commit, "--", "src/lib.rs"])
        b = r.hgit(["-c", "attr.tree=" + res.commit, "diff-tree", "-p", r.fx.base, res.commit, "--", "src/lib.rs"],
                   attr=False)
        res.notes.append("-diff: text diff with the empty attribute source: " + str(b"+pub fn changed" in a.out)
                         + "; hidden when the candidate's attributes apply: " + str(b"Binary files" in b.out))
        return bad
    return check


@case("agent-edited .gitignore", "candidate")
def _(r):
    w(r.p(".gitignore"), "*.log\nhidden/\n!build/shown.log\n")     # drops target/, adds hidden/, un-ignores one log
    w(r.p("hidden/tool.rs"), "exists in the workspace only\n")
    w(r.p("target/debug/app"), "no longer ignored\n")
    w(r.p("build/shown.log"), "un-ignored\n")
    w(r.p("vendor/self/.gitignore"), "*\n")                        # hides its own directory, itself included
    w(r.p("vendor/self/blob.rs"), "hidden without a trace\n")
    w(r.p("build/keep.log"), "tracked, still matches *.log, modified\n")

    def check(res):
        t = tree_entries(r, res.tree)
        want = {".gitignore": True, "hidden/tool.rs": False, "target/debug/app": True, "build/shown.log": True,
                "vendor/self/.gitignore": False, "vendor/self/blob.rs": False}
        res.notes.append("in tree: " + ", ".join(f"{p}={'yes' if p in t else 'no'}" for p in want))
        return [p for p, v in want.items() if (p in t) != v]
    return check


@case(".gitignore is a symlink to a file outside", "candidate")
def _(r):
    os.remove(r.p(".gitignore"))
    os.symlink(r.fx.outside + "/ignore-all", r.p(".gitignore"))
    w(r.p("src/new.rs"), "pub fn new() {}\n")

    def check(res):
        res.notes.append("status stderr: " + (res.status_stderr or "(none)"))
        return [] if has(res, r, "src/new.rs") else ["a symlinked .gitignore was followed"]
    return check


@case("large file (6 MiB, untracked)", "listing")
def _(r):
    w(r.p("big.bin"), os.urandom(6 * 2**20))


@case("large file in an ignored directory", "candidate")
def _(r):
    w(r.p("target/big.bin"), os.urandom(6 * 2**20))


@case("sparse file (1 TiB apparent size)", "listing")
def _(r):
    w(r.p("huge.bin"), "")
    os.truncate(r.p("huge.bin"), 2**40)


@case("change set over the total cap (11 files of 4.9 MiB)", "listing")
def _(r):
    for i in range(11):
        w(r.p(f"data/{i}.bin"), os.urandom(int(4.9 * 2**20)))


@case("case collision", "ingest", linux_only=True)
def _(r):
    w(r.p("readme.md"), "collides with README.md\n")
    w(r.p("src/Lib.rs"), "collides with lib.rs\n")


@case("names: newline, leading dash, tab, quote, non-UTF-8, 255 bytes", "candidate")
def _(r):
    names = ["new\nline.txt", "-rf", "--stdin", "tab\tname", 'quo"te', " lead", "x" * 255]
    if LINUX:
        names.append(os.fsdecode(b"\xff\xfe.bin"))
    for n in names:
        w(r.p("odd/" + n), "x\n")

    def check(res):
        t = tree_entries(r, res.tree)
        return [repr(n) for n in names if "odd/" + n not in t]
    return check


def run_cases(fx, only):
    rows = []
    for name, expect, fn, linux_only in CASES:
        if only and only not in name:
            continue
        if linux_only and not LINUX:
            rows.append((name, expect, "skipped (needs a case-sensitive file system)", True))
            continue
        print(f"\n== {name}")
        r = Run(fx, name)
        check = fn(r)
        try:
            res = capture(r, timeout=15)
        except Exception as e:  # prototype: report and carry on
            print("   ERROR", e)
            rows.append((name, expect, f"ERROR {e}", False))
            continue
        layers = [("1 scan", res.scan), ("2 listing", res.listrej), ("3 stage", res.stage), ("4 ingest", res.ingest)]
        print("   change set:", ", ".join(f"{s} {p!r}" for s, p in res.listing[:12]) or "(empty)",
              "…" if len(res.listing) > 12 else "")
        for label, v in layers:
            print(f"   {label}: " + ("ok" if not v else "REJECT " + "; ".join(v[:4])))
        bad = []
        if res.first() == "candidate":
            print(f"   tree {res.tree}  candidate {res.commit}")
            bad = check(res) if check else []
        for n in res.notes + ([r.note] if hasattr(r, "note") else []):
            print("   note:", n)
        fired = fx.fired()
        print("   canary:", "FIRED" if fired else "untouched")
        ok = res.first() == expect and not bad and not fired
        if bad:
            print("   UNEXPECTED:", bad)
        stopped = [label for label, v in layers if v]
        rows.append((name, expect, "stopped by " + ", ".join(stopped) if stopped else "candidate", ok))
        if fired:
            os.remove(fx.canary)
    return rows


# --------------------------------------------------------------------------
# Experiments that do not fit the four-layer shape
# --------------------------------------------------------------------------


def experiments(root, fx, only):
    rows = []

    def exp(name):
        if only and only not in name:
            return False
        print(f"\n== experiment: {name}")
        return True

    def row(name, verdict, ok=True):
        print("   =>", verdict)
        rows.append((name, "", verdict, ok))

    if exp("exact commands of a benign capture"):
        global VERBOSE
        keep, VERBOSE = VERBOSE, True
        r = Run(fx, "commands", show=True)
        w(r.p("src/lib.rs"), "pub fn changed() {}\n")
        w(r.p("src/new.rs"), "pub fn new() {}\n")
        os.remove(r.p("docs/guide.md"))
        res = capture(r)
        VERBOSE = keep
        again = commit(r, res.tree)
        row("commit-tree is repeatable", f"same inputs give {res.commit[:12]} twice: {again == res.commit}",
            again == res.commit)
        r2 = Run(fx, "commands-empty")
        res2 = capture(r2)
        row("empty change set", f"tree equals the previous tree: {res2.tree == r2.prev_tree}; layers: {res2.first()}",
            res2.tree == r2.prev_tree)

    if exp("staging with in-tree attributes honoured against the empty attribute source"):
        out = {}
        lab = lambda b: "empty tree" if b else "in-tree"
        for attr, eattr in ((True, True), (False, True), (False, False)):
            r = Run(fx, f"attr-{attr}-{eattr}")
            w(r.p(".gitattributes"), "crlf.txt text eol=crlf\nutf16.txt working-tree-encoding=UTF-16\nid.txt ident\n")
            w(r.p("crlf.txt"), b"one\r\ntwo\r\n")
            w(r.p("utf16.txt"), "héllo\n".encode("utf-16"))
            w(r.p("id.txt"), "$Id: 0123456789012345678901234567890123456789 $\n")
            res = capture(r, attr=attr, export_attr=eattr)
            out[attr, eattr] = res.notes[-1] if res.first() == "candidate" else res.first()
            print(f"   staging with {lab(attr)} attributes, export with {lab(eattr)}: {out[attr, eattr]}")
        row("only the empty attribute source at both ends reproduces the workspace", "; ".join(out.values()),
            [v == "export == workspace" for v in out.values()] == [True, False, False])

    if exp("base commit that carries attributes: does an untouched workspace show a change set?"):
        files = {".gitattributes": "crlf.txt text eol=crlf\nutf16.txt working-tree-encoding=UTF-16\n",
                 "crlf.txt": "one\ntwo\n", "utf16.txt": "héllo\n", "README.md": "x\n"}
        fa = Fixture(root, files, name="attrs")
        out = []
        for co, st in ((True, True), (False, False), (False, True), (True, False)):
            r = Run(fa, f"attrs-{co}-{st}", attr=co)
            prepare_quarantine(r)
            listing, _, _ = list_changes(r, attr=st)
            lab = lambda b: "empty-tree" if b else "in-tree"
            print(f"   checkout with {lab(co)} attributes, staging with {lab(st)}: change set {listing or '(empty)'}")
            out.append(bool(listing))
        row("workspace checkout and staging must use the same attribute source",
            f"same source, empty change set: {not out[0] and not out[1]}; in-tree checkout then empty-tree staging "
            f"shows untouched files as changed: {out[2]}; the reverse: {out[3]}", out[:3] == [False, False, True])

    if exp("export: checkout-index with and without the empty attribute source"):
        r = Run(fx, "export-attrs")
        w(r.p(".gitattributes"), "crlf.txt text eol=crlf\n")
        w(r.p("crlf.txt"), "one\ntwo\n")
        res = capture(r)
        d2, _ = export(r, res.commit, attr=False, name="with-in-tree-attributes")
        a = open(os.path.join(res.export, "crlf.txt"), "rb").read()
        b = open(os.path.join(d2, "crlf.txt"), "rb").read()
        print(f"   blob is b'one\\ntwo\\n'; export with GIT_ATTR_SOURCE=empty tree: {a!r}; without: {b!r}")
        row("checkout-index applies the candidate's own attributes unless told otherwise", f"{a != b}", a != b)

    if exp("naive staging: git add -A on a nested repository"):
        r = Run(fx, "naive-add")
        nested_repo(fx, r.p("vendor/nested"))
        prepare_quarantine(r)
        r.sgit(["read-tree", r.prev_tree])
        o = r.sgit(["add", "-A"])
        tree = r.sgit(["write-tree"]).text
        pack = r.sgit(["pack-objects", "--revs", "--stdout", "--local", "-q"], stdin=tree.encode() + b"\n").out
        rej = ingest(r, tree, pack)
        print("   add -A stderr:", o.errtext.splitlines()[0] if o.errtext else "(none)")
        row("git add -A turns a nested repository into a gitlink", f"ingest: {rej}; canary fired: {fx.fired()}",
            bool(rej) and not fx.fired())

    if exp("base tree with a gitlink, populated by a hostile nested repository"):
        fg = Fixture(root, {"README.md": "x\n"}, name="gitlink")
        up = os.path.join(root, "gitlink-upstream")
        g = lambda a: must(git(a, cwd=up, extra=IDENT, show=False))
        nested_repo(fx, os.path.join(fx.outside, "sub-head"))   # same content and dates, so the same commit as below
        head = must(git(["rev-parse", "HEAD"], cwd=os.path.join(fx.outside, "sub-head"), show=False)).text
        g(["update-index", "--add", "--cacheinfo", f"160000,{head},sub"])
        g(["commit", "-q", "-m", "gitlink"])
        fg.base = g(["rev-parse", "HEAD"]).text
        g(["push", "-q", fg.mirror, "main"])
        for flag in ("all", "none"):
            r = Run(fg, f"gitlink-{flag}")
            shutil.rmtree(r.p("sub"), ignore_errors=True)
            nested_repo(fx, r.p("sub"))
            prepare_quarantine(r)
            listing, errs, _ = list_changes(r, submodules=flag)
            fired = fx.fired()
            print(f"   diff-files --ignore-submodules={flag}: change set {listing or '(empty)'}; "
                  f"canary {'FIRED' if fired else 'untouched'}")
            if fired:
                os.remove(fx.canary)
            rows.append((f"gitlink in base, diff-files --ignore-submodules={flag}", "",
                         "nested fsmonitor ran" if fired else "nothing ran", True))

    if exp(".gitignore is a FIFO and the scan is skipped"):
        r = Run(fx, "fifo-ignore")
        os.remove(r.p(".gitignore"))
        os.mkfifo(r.p(".gitignore"))
        prepare_quarantine(r)
        listing, errs, _ = list_changes(r, timeout=5)
        row("git blocks on a FIFO named .gitignore", f"{errs or 'no hang'}")

    if exp("untracked FIFO and the scan is skipped: does git list it?"):
        r = Run(fx, "fifo-silent")
        os.mkfifo(r.p("pipe"))
        prepare_quarantine(r)
        listing, errs, _ = list_changes(r, timeout=5)
        row("git silently omits an untracked FIFO", f"change set {listing or '(empty)'}", not listing)

    if exp("subverted staging: hostile packs and tree names"):
        r = Run(fx, "hostile-pack")
        prepare_quarantine(r)
        q = lambda a, **k: git(["--git-dir=" + r.q] + a, show=False, **k)
        blob = q(["hash-object", "-w", "--stdin"], stdin=b"x\n").text
        mk = lambda spec: q(["mktree", "--missing"], stdin=spec.encode()).text
        pack = lambda t: q(["pack-objects", "--stdout", "-q"], stdin=f"{t}\n{blob}\n".encode()).out
        trees = {
            "tree with an entry named .git": mk(f"100644 blob {blob}\t.git\n"),
            "tree with an entry named .GIT": mk(f"100644 blob {blob}\t.GIT\n"),
            "tree with an entry named git~1": mk(f"100644 blob {blob}\tgit~1\n"),
            "tree with a gitlink": mk(f"160000 commit {fx.base}\tsub\n"),
            "tree with a .gitmodules symlink": mk(f"120000 blob {blob}\t.gitmodules\n"),
            "tree naming a blob that is not in the pack": mk(f"100644 blob {'1' * 40}\tmissing\n"),
            "tree naming another run's blob": mk(f"100644 blob {fx.other_blob}\tstolen\n"),
        }
        for label, t in trees.items():
            rej = ingest(r, t, pack(t))
            print(f"   {label}: {rej or 'ACCEPTED'}")
            rows.append((label, "ingest", "rejected" if rej else "ACCEPTED", bool(rej)))
        for label, (t, p) in {"truncated pack": (r.prev_tree, pack(trees["tree with a gitlink"])[:-5]),
                              "tree name that is not hex": ("HEAD; rm -rf /", b""),
                              "garbage in place of a pack": (r.prev_tree, b"not a pack")}.items():
            rej = ingest(r, t, p)
            print(f"   {label}: {rej or 'ACCEPTED'}")
            rows.append((label, "ingest", "rejected" if rej else "ACCEPTED", bool(rej)))
        left = os.listdir(os.path.join(r.host, "objects/pack"))
        row("host.git after ten rejected ingests", f"objects/pack holds {len(left)} files, as after the clone: "
            f"{sorted(f.split('.')[-1] for f in left)}", len(left) <= 3)

    if exp("another run's objects"):
        r = Run(fx, "other-run")
        in_mirror = git(["--git-dir=" + fx.mirror, "cat-file", "-e", fx.other_blob]).rc == 0
        in_ws = git(["cat-file", "-e", fx.other_blob], cwd=r.ws).rc == 0
        in_host = r.hgit(["cat-file", "-e", fx.other_blob]).rc == 0
        row("another run's blob", f"in the mirror: {in_mirror}; in a fresh workspace: {in_ws}; in host.git: {in_host}",
            in_mirror and not in_ws and not in_host)

    if not LINUX and exp("export of a case-colliding tree on a case-insensitive file system"):
        r = Run(fx, "collide-export")
        target = os.path.join(fx.outside, "collide-target")
        os.makedirs(target)
        blob = r.hgit(["hash-object", "-w", "--stdin"], stdin=b"written through the symlink\n").text
        link = r.hgit(["hash-object", "-w", "--stdin"], stdin=target.encode()).text
        sub = r.hgit(["mktree"], stdin=f"100644 blob {blob}\tpwned\n".encode()).text
        tree = r.hgit(["mktree"], stdin=f"120000 blob {link}\tA\n040000 tree {sub}\ta\n".encode()).text
        dest, o = export(r, tree, name="collide")
        escaped = os.path.exists(os.path.join(target, "pwned"))
        print("   checkout-index:", o.errtext or "(no error)", "| export holds", os.listdir(dest))
        row("symlink A and directory a in one tree", f"file written outside the export: {escaped}", not escaped)

    return rows


def main():
    only = sys.argv[sys.argv.index("-k") + 1] if "-k" in sys.argv else None
    root = os.path.realpath(tempfile.mkdtemp(prefix="sf-p7-", dir=os.environ.get("SF_PROTO_TMP")))
    print(f"git {git(['version'], show=False).text.split()[-1]} on {sys.platform}, "
          f"sandbox: {'bwrap' if BWRAP else 'none'}, scratch: {root}")
    fx = Fixture(root)
    rows = run_cases(fx, only) + experiments(root, fx, only)
    print("\n\n| Case | Must stop at | Observed | As expected |\n|---|---|---|---|")
    for name, expect, got, ok in rows:
        print(f"| {name} | {expect} | {got} | {'yes' if ok else 'NO'} |")
    sys.exit(0 if all(r[3] for r in rows) else 1)


if __name__ == "__main__":
    main()
