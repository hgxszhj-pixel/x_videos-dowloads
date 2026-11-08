#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""插件加载器（带热加载）—— 功能以可插拔方式长在 exam-lookup 上，主程序永远不被插件拖死。

约定：plugins/<name>.py 提供模块级 PLUGIN 字典

    PLUGIN = {
        "name": "srs",                                  # 必须与文件名一致
        "desc": "错题复习调度（FSRS-6）",
        "routes": {"/api/srs/due": handler},            # handler(query: dict) -> 可 JSON 序列化对象
        "selftest": fn,                                 # 可选；失败抛 AssertionError
    }

热加载：每次取插件（load / routes / 每分钟的 webui 请求）都会 stat 一遍文件，
mtime 或大小变了就重新 exec 这个模块 —— 改插件不用重启 webui，也不用重启服务进程。

崩溃隔离（硬性要求「避免项目崩溃」）：
  1. 导入失败 → 记 error，保留上一次能用的版本（新代码写坏了，老功能继续跑）
  2. 路由内部抛异常 → guard() 转成 {"error": ...}，只影响这一次请求
  3. 并发 → 一把锁保护重载，两个线程同时发现变更也只会重载一次
"""
import importlib.util
import os
import threading
import time
import traceback

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
STAT_TTL = 1.0                    # 同一秒内多次请求只 stat 一次，别让每个请求都碰磁盘

_lock = threading.RLock()
_state = {
    "good": {},                   # name -> {"plugin", "mtime", "size", "loaded_at"}
    "errors": {},                 # name -> "ExceptionType: msg"
    "checked_at": 0.0,
    "reloads": 0,
}


def _plugin_files():
    return [f for f in sorted(os.listdir(HERE))
            if f.endswith(".py") and not f.startswith("_")]


def _import_one(path, name):
    """按文件路径导入，不依赖 sys.path（webui 从任何 cwd 启动都能用）。"""
    spec = importlib.util.spec_from_file_location("exam_plugin_" + name, path)
    if spec is None or spec.loader is None:
        raise ImportError("无法加载 %s" % path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def _validate(mod, name):
    p = getattr(mod, "PLUGIN", None)
    if not isinstance(p, dict):
        raise ValueError("缺少模块级 PLUGIN 字典")
    if (p.get("name") or name) != name:
        raise ValueError("PLUGIN['name']=%r 与文件名 %r 不一致" % (p.get("name"), name))
    if not isinstance(p.get("routes") or {}, dict):
        raise ValueError("routes 必须是 dict")
    for path in (p.get("routes") or {}):
        if not str(path).startswith("/api/"):
            raise ValueError("路由必须以 /api/ 开头：%r" % path)
    p.setdefault("name", name)
    p.setdefault("desc", "")
    return p


def _reload_one(name, fn, path):
    """重新加载单个插件。失败时保留旧版本（这是热加载最要紧的一条）。"""
    old = _state["good"].get(name)
    try:
        p = _validate(_import_one(path, name), name)
    except Exception as e:                     # noqa: BLE001 —— 插件坏了不许牵连主程序
        _state["errors"][name] = "%s: %s" % (type(e).__name__, e)
        return old is not None                 # True = 还有旧版本可用
    st = os.stat(path)
    _state["good"][name] = {"plugin": p, "mtime": st.st_mtime, "size": st.st_size,
                            "loaded_at": time.strftime("%Y-%m-%d %H:%M:%S")}
    _state["errors"].pop(name, None)
    if old is not None:
        _state["reloads"] += 1
    return True


def load(force=False):
    """返回 (plugins: {name: PLUGIN}, errors: {name: 原因})。任何异常都在这里被吃掉。"""
    with _lock:
        now = time.time()
        if not force and now - _state["checked_at"] < STAT_TTL:
            return ({n: v["plugin"] for n, v in _state["good"].items()}, dict(_state["errors"]))
        _state["checked_at"] = now
        names = set()
        for fn in _plugin_files():
            name = fn[:-3]
            names.add(name)
            path = os.path.join(HERE, fn)
            try:
                st = os.stat(path)
            except OSError as e:
                _state["errors"][name] = "OSError: %s" % e
                continue
            cur = _state["good"].get(name)
            if force or cur is None or cur["mtime"] != st.st_mtime or cur["size"] != st.st_size:
                _reload_one(name, fn, path)
        for gone in set(_state["good"]) - names:       # 文件被删：插件下线（不留假报错）
            _state["good"].pop(gone, None)
            _state["errors"].pop(gone, None)
        return ({n: v["plugin"] for n, v in _state["good"].items()}, dict(_state["errors"]))


def routes():
    """合并所有插件的 GET 路由；键冲突时先加载的胜出，并在 errors 里留痕。"""
    plugins, errors = load()
    out = {}
    for name, p in plugins.items():
        for path, fn in (p.get("routes") or {}).items():
            if path in out:
                errors.setdefault("_route_" + name, "路由 %s 与已加载插件冲突，已跳过" % path)
                continue
            out[path] = fn
    return out


def status():
    """给 /api/plugins 和 CLI 看的自检视图：谁在线、版本时间、有没有报错。"""
    load()
    with _lock:
        return {
            "plugins": [{"name": n, "desc": v["plugin"].get("desc", ""),
                         "routes": sorted((v["plugin"].get("routes") or {})),
                         "mtime": time.strftime("%Y-%m-%d %H:%M:%S", time.localtime(v["mtime"])),
                         "loaded_at": v["loaded_at"]} for n, v in _state["good"].items()],
            "errors": dict(_state["errors"]),
            "reloads": _state["reloads"],
            "checked_at": time.strftime("%Y-%m-%d %H:%M:%S", time.localtime(_state["checked_at"])),
        }


def guard(fn):
    """把插件路由的异常转成结构化错误，绝不让它冒到 HTTP 层把服务带崩。"""
    def wrapper(query=None):
        try:
            return fn(query or {})
        except Exception as e:                     # noqa: BLE001
            return {"error": "%s: %s" % (type(e).__name__, e),
                    "traceback": traceback.format_exc(limit=3)}
    return wrapper


def selftest():
    """红/绿自检：发现插件 / 坏插件隔离 / 热加载生效 / 写坏了能回滚 / guard 兜异常。"""
    plugins, errors = load(force=True)
    assert "srs" in plugins, "srs 插件应被加载：%r" % (errors,)
    for name, p in plugins.items():
        for path in (p.get("routes") or {}):
            assert path.startswith("/api/"), "%s 的路由必须以 /api/ 开头：%s" % (name, path)

    # 1) 坏插件只被记为 error，不抛异常
    bad = os.path.join(HERE, "zz_selftest_broken.py")
    with open(bad, "w", encoding="utf-8") as f:
        f.write("raise RuntimeError('插件自己炸了')\n")
    try:
        _, e1 = load(force=True)
        assert "zz_selftest_broken" in e1, "坏插件应被记为加载失败：%r" % (e1,)
        assert "srs" in load()[0], "坏插件不该把正常插件挤掉"
    finally:
        os.remove(bad)
        load(force=True)

    # 2) 热加载：改文件 + 前进 mtime → 不重启进程就能拿到新代码
    probe = os.path.join(HERE, "zz_selftest_hot.py")
    tpl = '''PLUGIN = {"name": "zz_selftest_hot", "desc": %r,
        "routes": {"/api/zz_selftest_hot": lambda q: {"v": %r}}}
'''
    try:
        with open(probe, "w", encoding="utf-8") as f:
            f.write(tpl % ("v1", "v1"))
        load(force=True)
        assert load()[0]["zz_selftest_hot"]["desc"] == "v1", "首版应加载 v1"
        os.utime(probe, (time.time() + 2, time.time() + 2))   # 模拟编辑器保存
        with open(probe, "w", encoding="utf-8") as f:
            f.write(tpl % ("v2", "v2"))
        os.utime(probe, (time.time() + 4, time.time() + 4))
        _state["checked_at"] = 0                              # 越过 stat 缓存
        p2, _ = load()
        assert p2["zz_selftest_hot"]["desc"] == "v2", "热加载没生效：仍拿到 %r" % p2["zz_selftest_hot"]["desc"]
        assert p2["zz_selftest_hot"]["routes"]["/api/zz_selftest_hot"]({})["v"] == "v2"

        # 3) 写坏了（语法错误）→ 保留上一版能用，error 里留痕
        with open(probe, "w", encoding="utf-8") as f:
            f.write("def broken(:\n")
        os.utime(probe, (time.time() + 6, time.time() + 6))
        _state["checked_at"] = 0
        p3, e3 = load()
        assert "zz_selftest_hot" in e3, "语法错误的插件应记 error"
        assert p3["zz_selftest_hot"]["desc"] == "v2", "新版写坏了必须回退到上一版（v2），实际 %r" \
            % (p3.get("zz_selftest_hot") or {}).get("desc")
    finally:
        if os.path.exists(probe):
            os.remove(probe)
        load(force=True)

    # 4) guard：插件抛异常 -> 结构化错误，而不是异常上抛
    def boom(_q):
        raise ValueError("故意炸")
    r = guard(boom)({})
    assert str(r.get("error", "")).startswith("ValueError"), "guard 应把异常转成 error 字段：%r" % r
    return plugins, errors
