# Which spellings of a file name the file system of a directory takes for one
# name: creates the first, asks whether the second exists. Usage:
#   python3 -I apfs.py <empty directory>
# Throwaway code for probe P-29.
import os, sys, shutil, unicodedata
base = sys.argv[1]
pairs = [
 ("ascii case", "AGENTS.md", "agents.MD"),
 ("latin1 case", "É.md", "é.md"),
 ("NFC vs NFD", "é.md", "é.md"),
 ("kelvin sign", "workflows", "worKflows"),
 ("long s", "AGENTS.md", "AGENTſ.md"),
 ("sharp s full fold", "straße", "STRASSE"),
 ("capital sharp s", "straße", "STRAẞE"),
 ("final sigma", "σ", "ς"),
 ("capital sigma vs final", "Σ", "ς"),
 ("dz titlecase", "ǆ", "ǅ"),
 ("dotted capital I", "i", "İ"),
 ("dotless i", "I", "ı"),
 ("fi ligature (compat)", "fi", "ﬁ"),
 ("fullwidth a (compat)", "a", "ａ"),
 ("ZWNJ ignorable", ".git2", ".g‌it2"),
 ("BOM ignorable", ".git3", ".git3﻿"),
 ("iota subscript full fold", "ᾀ", "ἀι"),
 ("ypogegrammeni", "ͅx", "ιx"),
 ("cherokee", "Ꭰ", "ꭰ"),
 ("trailing dot", "name", "name."),
 ("trailing space", "name2", "name2 "),
]
for label, a, b in pairs:
    d = os.path.join(base, "t")
    shutil.rmtree(d, ignore_errors=True); os.mkdir(d)
    with open(os.path.join(d, a), "w") as f: f.write("x")
    same = os.path.exists(os.path.join(d, b))
    listed = os.listdir(d)
    print(f"{label:26} {a.encode('unicode_escape').decode():22} {b.encode('unicode_escape').decode():24} same_name={same} stored={[x.encode('unicode_escape').decode() for x in listed]}")
shutil.rmtree(os.path.join(base, "t"), ignore_errors=True)
