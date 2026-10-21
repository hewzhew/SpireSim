# 原始 corpus 覆盖版本回归

这里逐字节保留一个自然 Neow `Proceed` 的前后边界、动作收据、原始 bootstrap 和录制时的
corpus 清单。[sources.json](sources.json) 记录来源、Java producer 与每个文件的内容摘要。
原文件没有删字段或改版本；这是最短的既有单动作 corpus case，不是整局或整幕认证。

清单中保存的是录制时的地图与 RNG 投影版本。测试必须用当前 Java projection、Rust 与
逐转换 RNG 调用账本重新比较，不能把旧覆盖说明与当前说明逐字相等作为前提。去重也必须
使用重新计算的当前覆盖，并确认保留项当前仍 Exact；原始清单与边界保持不变。

```powershell
cargo test --locked -p sts_parity_lockstep_runner --lib corpus::
```

测试先验证原字节及 producer，再将副本放入隔离的临时 corpus。负例只修改临时副本：错误
收据、producer 变化、真实金币差异与缺失字段仍须停止。所有测试离线执行，不依赖本机
`.oracle-lab`、游戏安装或 Java 进程。这些记录不携带存档，也不获得恢复或执行权限。
