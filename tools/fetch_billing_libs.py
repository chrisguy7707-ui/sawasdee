#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Downloads Google Play Billing and the libraries it needs from Google's Maven (dl.google.com)
into android/libs/ (git-ignored). Gradle is not used here, so the dependency list is resolved by
reading each library's .pom file. Run with --dry-run to only list what would be downloaded."""
import os, re, sys, urllib.request, xml.etree.ElementTree as ET

ROOT = "https://dl.google.com/dl/android/maven2/"
START = ("com.android.billingclient", "billing", "9.1.0")
OUT = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "android", "libs")
NS = {"m": "http://maven.apache.org/POM/4.0.0"}

def url(g, a, v, ext):
    return f"{ROOT}{g.replace('.', '/')}/{a}/{v}/{a}-{v}.{ext}"

def get(u):
    with urllib.request.urlopen(u, timeout=60) as r:
        return r.read()

def deps(g, a, v):
    root = ET.fromstring(get(url(g, a, v, "pom")))
    out = []
    for d in root.findall(".//m:dependencies/m:dependency", NS):
        gg, aa, vv = (d.findtext("m:" + k, namespaces=NS) for k in ("groupId", "artifactId", "version"))
        scope = d.findtext("m:scope", default="compile", namespaces=NS)
        if scope in ("compile", "runtime") and vv:
            out.append((gg, aa, vv.strip("[]")))
    return out

def main():
    dry = "--dry-run" in sys.argv
    seen, todo, order = {}, [START], []
    while todo:
        g, a, v = todo.pop(0)
        if (g, a) in seen:
            continue
        seen[(g, a)] = v
        order.append((g, a, v))
        try:
            todo += deps(g, a, v)
        except Exception as e:
            print("pom problem:", g, a, v, e)
    os.makedirs(OUT, exist_ok=True)
    for g, a, v in order:
        # prefer .aar, fall back to .jar
        for ext in ("aar", "jar"):
            u = url(g, a, v, ext)
            try:
                req = urllib.request.Request(u, method="HEAD")
                size = int(urllib.request.urlopen(req, timeout=60).headers.get("Content-Length", 0))
            except Exception:
                continue
            print(f"{g}:{a}:{v}.{ext}  {size/1024:.0f} KB")
            if not dry:
                open(os.path.join(OUT, f"{a}-{v}.{ext}"), "wb").write(get(u))
            break
        else:
            print(f"NOT ON GOOGLE MAVEN: {g}:{a}:{v}")
main()
