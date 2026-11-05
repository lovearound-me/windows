#!/usr/bin/env python3
"""Checks the dictionaries against the Windows app: every text it passes to
tr()/trn() is in every language, and nothing is left that it doesn't use.

    python3 i18n/check.py            # what is missing or unused, per language
    python3 i18n/check.py --keys     # every key the app uses, with where

Keys are the Russian texts. ru.json holds only the plural forms (trn), since
for everything else the key is the Russian text itself.
"""
import ast
import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
HERE = os.path.join(ROOT, "i18n")
LANGS = ["en", "es", "pt", "de", "fr", "it", "pl", "tr", "uk", "ru", "id", "ko", "zh"]
CATEGORIES = {
    "ru": {"one", "few", "many"}, "uk": {"one", "few", "many"}, "pl": {"one", "few", "many"},
    "id": {"other"}, "ko": {"other"}, "zh": {"other"},
}
PLACEHOLDER = re.compile(r"\{(\w+)\}")


def unescape(text, quote):
    return text.replace("\\" + quote, quote).replace("\\n", "\n").replace("\\\\", "\\")


def python_keys(path):
    tree = ast.parse(open(path, encoding="utf-8").read())
    for node in ast.walk(tree):
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Name) and node.func.id in ("tr", "trn", "N_"):
            if node.args and isinstance(node.args[0], ast.Constant) and isinstance(node.args[0].value, str):
                yield node.args[0].value, node.func.id == "trn", node.lineno


CALL = {
    ".js": re.compile(r"\b(trn?)\('((?:[^'\\\n]|\\.)*)'"),
    ".rs": re.compile(r"\b(tr)!\(\"((?:[^\"\\\n]|\\.)*)\""),
    ".swift": re.compile(r"\b(trn?)\(\"((?:[^\"\\\n]|\\.)*)\""),
}


def source_keys(path, ext):
    quote = "'" if ext == ".js" else '"'
    for n, line in enumerate(open(path, encoding="utf-8"), 1):
        if line.lstrip().startswith("//"):
            continue
        for m in CALL[ext].finditer(line):
            yield unescape(m.group(2), quote), m.group(1) == "trn", n


def used():
    keys = {}
    dirs = [("ui", ".js"), ("src-tauri/src", ".rs")]
    for folder, ext in dirs:
        for base, _, files in os.walk(os.path.join(ROOT, folder)):
            for name in sorted(files):
                if not name.endswith(ext) or name in ("i18n.py", "i18n.rs", "Strings.swift"):
                    continue
                path = os.path.join(base, name)
                found = python_keys(path) if ext == ".py" else source_keys(path, ext)
                for key, plural, line in found:
                    entry = keys.setdefault(key, {"plural": plural, "where": []})
                    entry["plural"] |= plural
                    entry["where"].append(f"{os.path.relpath(path, ROOT)}:{line}")
    return keys


def main():
    keys = used()
    if "--keys" in sys.argv:
        for key, entry in sorted(keys.items()):
            print(("[n] " if entry["plural"] else "    ") + json.dumps(key, ensure_ascii=False), entry["where"][0])
        return 0
    problems = 0
    for lang in LANGS:
        try:
            table = json.load(open(os.path.join(HERE, lang + ".json"), encoding="utf-8"))
        except (OSError, ValueError) as error:
            print(f"{lang}: {error}")
            problems += 1
            continue
        wanted = {k for k, e in keys.items() if e["plural"]} if lang == "ru" else set(keys)
        missing = sorted(wanted - set(table))
        unused = sorted(set(table) - set(keys))
        bad = []
        for key in sorted(wanted & set(table)):
            value = table[key]
            if keys[key]["plural"]:
                need = CATEGORIES.get(lang, {"one", "other"})
                if not isinstance(value, dict) or not need <= set(value):
                    bad.append(f"{key!r}: plural forms {sorted(need)}")
                    continue
                forms = value.values()
            else:
                if not isinstance(value, str):
                    bad.append(f"{key!r}: not a string")
                    continue
                forms = [value]
            for form in forms:
                have, want = set(PLACEHOLDER.findall(form)), set(PLACEHOLDER.findall(key))
                # A plural form may leave out {n} ("one hour ago"); others keep every one.
                if (not have <= want) if keys[key]["plural"] else have != want:
                    bad.append(f"{key!r}: placeholders in {form!r}")
        for title, items in (("missing", missing), ("unused", unused), ("wrong", bad)):
            for item in items:
                print(f"{lang} {title}: {item}")
            problems += len(items)
    print(f"{len(keys)} keys, {problems} problems")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
