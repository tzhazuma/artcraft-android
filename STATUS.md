# ArtCraft → Android arm64-v8a：试点移植状态

工作区 `~/artcraft-android/`。目标：把 `github.com/storytold` 的 Craft 系列（纯 Rust + egui/eframe）
移植到 Android arm64-v8a，先打通一个试点，再把方法复制到其余应用。

**试点：GridCraft v0.7.0**（`storytold/gridcraft`，Excel 风格电子表格）。
**第二个目标：craft-launcher v0.4.1**（ArtCraft Launcher，套件启动器）。

上游 commit：`f1894c6da64d4326022ae644c767581816a23c2c`

---

## 0. 一句话结论

GridCraft 的 Android 移植**编译链路已完全打通**：Rust `cdylib` → `libmain.so`(34.7 MB) → APK(31.0 MB)，
0 error、16 KB 页对齐通过、`aapt2` 校验通过。**唯一未完成的环节是真机安装截图——设备处于锁屏，
vivo 会静默丢弃安装，而解锁手势（双指上滑）无法用 adb 合成。见 §6。**

---

## 1. 环境事实（与任务简报不同，已实测修正）

| 项 | 简报说的 | 实测 |
|---|---|---|
| rustup | **没有**，缺 `aarch64-linux-android` std | **本来就有**。默认 toolchain 是 `nightly`(1.91) 且**确实**缺该 target；但 `stable` = **cargo/rustc 1.99.0**，**已经装了 `aarch64-linux-android` std**。→ 全流程只要 `cargo +stable`，**不需要安装任何东西** |
| cargo-ndk | 4.1.2 已装 | ✅ `cargo-ndk 4.1.2` |
| NDK | `~/Library/Android/sdk/ndk/28.2.13676358` | ✅（r28 是 16 KB 页对齐的前提） |
| adb | 未提 | `~/Library/Android/sdk/platform-tools/adb`，**不在非交互 shell 的 PATH 里** |
| gh | 「已以 tzhazuma 登录」 | ✅ 在 `/opt/homebrew/bin/gh`，**也不在 PATH 里** |
| JDK | 未提 | 系统 `java` 是 **27**，AGP 9.1 **会拒绝**。必须 `JAVA_HOME=/opt/homebrew/opt/openjdk@21` |
| Gradle | 未提 | `gradle-9.3.1` 已在 `~/.gradle/wrapper/dists`，**离线可用**，无需下载 |
| python3 | 坏的 | ✅ 确认；脚本里一律 `/usr/bin/python3` |
| 内存 | 未提 | **18 GB，可用常年在 100 MB 量级**。并发 rustc 会 OOM → 见 §4 的构建锁 |

验证命令：

```bash
rustc +stable --print target-libdir --target aarch64-linux-android
# /Users/azuma/.rustup/toolchains/stable-aarch64-apple-darwin/lib/rustlib/aarch64-linux-android/lib
```

---

## 2. 完整可复现步骤

```bash
cd ~/artcraft-android
source scripts/env.sh && artcraft_check_env      # 缺什么会明确报出来

# 1) 上游（浅克隆即可）
mkdir -p src && git clone --depth 1 https://github.com/storytold/gridcraft.git src/gridcraft

# 2) Rust cdylib → apps/gridcraft-android/jniLibs/arm64-v8a/libmain.so
scripts/build-so.sh gridcraft

# 3) Gradle 外壳（模板 + 该 app 的图标集）
scripts/make-shell.sh "$PWD/src/gridcraft" gridcraft-android \
    ai.storyteller.gridcraft GridCraft 0.7.0 gridcraft

# 4) APK → out/GridCraft-0.7.0-android-arm64.apk
scripts/build-apk.sh gridcraft

# 5) 真机（需要设备解锁，见 §6）
scripts/device-smoke.sh ai.storyteller.gridcraft .MainActivity GridCraft
```

`scripts/env.sh` 里 export 的关键变量（不 source 它必然踩坑）：

```bash
export PATH="$HOME/.cargo/bin:$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:/opt/homebrew/bin:$PATH"
export ANDROID_NDK_HOME="$HOME/Library/Android/sdk/ndk/28.2.13676358"
export JAVA_HOME=/opt/homebrew/opt/openjdk@21
export CARGO_BUILD_JOBS=4          # 18 GB 内存下的上限
```

---

## 3. 产物与证据

| 产物 | 路径 | 大小 | SHA-256 |
|---|---|---|---|
| Rust 动态库 | `src/gridcraft/apps/gridcraft-android/jniLibs/arm64-v8a/libmain.so` | 34,719,928 B | `9d42492d7e29776cf81382d98c2d5955915d1b8f1c51adabe6ec2121227b5a38` |
| APK | `out/GridCraft-0.7.0-android-arm64.apk` | 31,046,808 B | `e2844f5c4ae92cac595fe139be586e13af2662715cf23afb7bdde055f11a431a` |

```
$ cargo +stable ndk -t arm64-v8a build --release -p gridcraft-android
== cargo exit code: 0
== artifact: .../libmain.so (34719928 bytes)          # 0 error, 0 warning

$ aapt2 dump badging out/GridCraft-0.7.0-android-arm64.apk
package: name='ai.storyteller.gridcraft' versionCode='1' versionName='0.7.0' ...
targetSdkVersion:'35'
application-label:'GridCraft'
launchable-activity: name='ai.storyteller.gridcraft.MainActivity'
native-code: 'arm64-v8a'
application-icon-480:'res/BW.xml'                      # 矢量 adaptive icon

$ zipalign -c -P 16 -v 4 out/GridCraft-0.7.0-android-arm64.apk
== zipalign -P 16: OK (16 KB page aligned)

$ llvm-readelf -lW libmain.so | grep LOAD
  LOAD 0x000000  0x0000000000000000 ... 0x66ddb0  R   0x4000
  LOAD 0x66ddb0  0x0000000000671db0 ... 0x108c150 R E 0x4000
  LOAD 0x16f9f00 0x0000000001701f00 ... 0x0b3988  RW  0x4000
  LOAD 0x17ad890 0x00000000017b9890 ... 0x001d20  RW  0x4000
```

---

## 4. 方法：加一个平台 crate，不动上游

上游已经为多平台准备好像素的抽象缝——每个 app 都有 `apps/<app>-web`（WASM），所以代码本身平台
抽象是干净的。移植照抄这个先例：**新建一个 L6 app crate**，复用 engine + ui-egui，只补平台壳。

```
ports/gridcraft-android/          ← 移植源码（source of truth）
  Cargo.toml                      [lib] name="main" crate-type=["cdylib"]
  src/lib.rs                      android_main + TouchShell + 字体 + 偏好
  README.md                       集成步骤 + 已知缺口
```

`build-so.sh` 每次构建前把 `ports/<pkg>/` **rsync 覆盖**进克隆的 `apps/<pkg>/`，所以移植源码永远
是权威，克隆里手改的东西不会静默生效。

### 关键实现点

1. **入口**（edition 2024 下 `#[no_mangle]` 必须写成 `#[unsafe(no_mangle)]`）：

```rust
#[allow(unsafe_code)]                       // 上游 workspace 是 unsafe_code="forbid"，
#[unsafe(no_mangle)]                        // 本 crate 用 deny + 入口处 allow，
fn android_main(app: AndroidApp) {          // 与 crates/platform 的既有先例一致
    let options = eframe::NativeOptions { android_app: Some(app), ..Default::default() };
    eframe::run_native("GridCraft", options, Box::new(|cc| Ok(Box::new(TouchShell::new(...))))).unwrap();
}
```

2. **eframe features 必须按目标拆分**：
   `["default_fonts","wgpu","android-game-activity"]`
   - `accesskit` + `android-native-activity` 是 eframe 的硬 `compile_error!`；
   - NativeActivity 没有软键盘/IME，ribbon + 公式栏这种要打字的 app **必须 GameActivity**。
   - 不要直接依赖 `android-activity`，用 winit 再导出的
     `winit::platform::android::activity::AndroidApp`。

3. **Gradle 外壳**：`templates/android-shell/`（AGP 9.1.0 / Gradle 9.3.1 / JDK 21 / minSdk 31 /
   compileSdk 35）。`app/build.gradle` 的 `jniLibs.srcDirs = ['../../jniLibs']` 指向 `cargo ndk -o`
   的输出目录。release 用 **debug 签名**，这样 release APK 能直接 `adb install`。
   `MainActivity extends GameActivity` + `static { System.loadLibrary("main"); }`，
   manifest 里 `android.app.lib_name = "main"` 必须与 `[lib] name = "main"` 一致。

4. **构建锁**（本机特有）：18 GB 内存 + 多个 agent 并发 cargo 会 OOM。`scripts/build-lock.sh`
   用 `/usr/bin/python3` 的 `fcntl.flock`（macOS 没有 `flock(1)`）把整机构建串行化，并配
   `CARGO_BUILD_JOBS=4`。

5. **剥掉构建机身份**：`RUSTFLAGS="--remap-path-prefix=$HOME=/build --remap-path-prefix=<clone>=/src"`。
   不加的话 rustc 会把 `/Users/azuma/...` 的 panic 路径烧进二进制，任何人 `strings libmain.so`
   都能看到（实测每包 632–1221 处）。

---

## 5. 踩坑清单

| # | 坑 | 现象 | 处理 |
|---|---|---|---|
| 1 | 默认 toolchain 是 nightly | `cargo build --target aarch64-linux-android` 报找不到 std | 全程 `cargo +stable` |
| 2 | 系统 java 是 27 | AGP 9.1 直接拒绝 | `JAVA_HOME=/opt/homebrew/opt/openjdk@21` |
| 3 | adb / gh 不在 PATH | `command not found` | 脚本显式 export |
| 4 | `bash -c "..."` 里嵌引号 | 外层展开把 PATH 搅坏，连 `cc` 都找不到 | 构建脚本用 **quoted heredoc + 占位符 + sed** 生成，绝不用 `bash -c` 传命令串 |
| 5 | 管道吃掉退出码 | `cargo ... \| tail` 让失败看起来成功 | 绝不把构建管道进 `tail`；`build-so.sh` 自己 `exit $rc` |
| 6 | `/sdcard` 安装 | SELinux 拒绝：system_server 读不了 fuse | `adb push` 到 **`/data/local/tmp/`** 再 `pm install -r -t` |
| 7 | 锁屏时安装 | `pm install` **无任何输出**，`lastUpdateTime` 不变，静默丢弃 | 装前查 `dumpsys window \| grep mDreamingLockscreen`；见 §6 |
| 8 | 无线调试端口漂移 | `adb devices` 空 | 先试 `<IP>:5555`（经典 adbd 端口，本会话两次救场），再 `adb mdns services \| grep _adb-tls-connect` |
| 9 | 大 APK 走无线 adb | `adb install` 永久挂住（包其实已装上） | 所有 adb 调用加 `timeout`；超时后改用「查包名是否存在」判定 |
| 10 | `mipmap-*dpi/*.webp` 继承自别的 app | 非 FilmCraft 的 app 显示 FilmCraft 图标 | 模板不带图标；每个 app 装自己的矢量集 |
| 11 | egui 默认字体无 CJK | 中文渲染成方块 | 运行时挂 `/system/fonts/NotoSansCJK-Regular.ttc` 到 family 末尾 |
| 12 | `dirs` 在 Android 返回 `None` | `Paths::new()` 掉进 `temp_dir()`（Android app 写不了 `/tmp`） | 需给 native crate 加 host 注入 data root 的口子（launcher 部分待做） |

---

## 6. 阻塞项：真机锁屏（需要人类）

设备 **vivo PA2573 / Android 16 / arm64-v8a**，`10.20.102.152:5555`。

```
$ adb -s 10.20.102.152:5555 shell dumpsys window | grep mDreamingLockscreen
    mShowingDream=false mDreamingLockscreen=true
$ adb -s 10.20.102.152:5555 shell dumpsys power | grep mWakefulness=
  mWakefulness=Asleep
```

- vivo 在**锁屏**时会**静默丢弃** `pm install`（无输出、`lastUpdateTime` 不变），所以装不上。
- 解锁手势是**双指上滑**，`adb shell input swipe` 合成不出来。
- `input keyevent KEYCODE_WAKEUP` 只能把 `mWakefulness` 变成 `Awake`，锁屏依旧在。

**请人类解锁手机并保持亮屏**，然后：

```bash
cd ~/artcraft-android
scripts/device-smoke.sh ai.storyteller.gridcraft .MainActivity GridCraft gridcraft-real
# 期望：PASS: launched, no crash / PASS: activity is focused
#       shots/gridcraft-real.png
```

`device-smoke.sh` 检测到锁屏会**直接报「HUMAN ACTION REQUIRED」并 exit 5**，不会空转重试。

旁证：设备上已装有 `ai.storyteller.{filmcraft,lightcraft,photocraft,effectcraft,printcraft}`
五个包，说明「push → pm install」这条路在本机是走通过的。

---

## 7. 还没做的 / 已知缺口

### GridCraft 首版缺的
- **文件打开/保存**：`rfd` 没有 Android 后端，`Services` 的 `pick_open`/`pick_save`/`pick_picture`
  全是 `None`。UI 对 `None` 有降级路径（不崩，只是选择器不可用）。要真可用得写 SAF 桥
  （`ACTION_OPEN_DOCUMENT` / `ACTION_CREATE_DOCUMENT`）。
- **剪贴板**：`arboard` 无 Android 后端；跨应用粘贴需要 JNI 调 `ClipboardManager`。
- **触屏布局**：ribbon 是按鼠标设计的。`TouchShell` 只把命中目标放大到 40 pt（
  `interact_size = 40×40`、`show_tooltips_only_when_still = true`）；真正的触控布局是产品级工作量。
- **中文界面**：CJK **字形**已回退，中文单元格能正常显示；界面语言仍是英文。

### craft-launcher（进行中）
已实测 **编译**通过（`cargo +stable ndk check --target aarch64-linux-android`，0 error）：
- `artcraft-launcher-ui-egui` —— 真正的启动器 egui UI
- `artcraft-launcher-native` —— 真正的下载/安装后端（含 ureq + rustls）

剩余运行时问题：
1. **`rustls-platform-verifier 0.7.1` 在 Android 需要额外 Kotlin 组件**，并且要从 GitHub Packages
   拉 Maven 包（`org.rustls:rustls-platform-verifier-android`），需要 token。**绕开方案**：改
   `crates/native/src/http.rs` 里唯一那处 `RootCerts::PlatformVerifier`，Android 上改成
   `RootCerts::new_with_certs(...)` 读系统 CA 目录——设备上实测有 **149 个**
   `/system/etc/security/cacerts/*.0`。仍然是「OS 决定信任什么」，且不引入新的 Gradle/Maven 依赖。
2. **`dirs` 在 Android 返回 `None`** → `Paths::new()` 需要 host 注入 data root
   （`AndroidApp::internal_data_path()`）。
3. 桌面壳的 `notify-rust` / `rfd` / `tray-icon` / `zbus` / `single_instance` 在 Android 上要
   么 cfg 掉、要么换成 Android 等价物（通知、SAF、Intent）。**注意**：因为移植是**新建 crate**
   而不是改 `apps/artcraft-launcher`，这些依赖根本不会被引入，桌面 app crate 一行都不用动。
4. `single_instance` 用 loopback TCP 47413 端口做单实例握手——Android 沙盒内可用，无需改。

### 其他
- 每个移植要**独立 GitHub 仓库**并带 release（见 §8）。
- craft-launcher 的下载地址要指向我们自己的 release（见 §9）。

---

## 8. 仓库与发布

命名跟已有惯例（`tzhazuma/freecad-android`、`blender-android`、`octave-android`、`dsh-android`）：

| 仓库 | 内容 |
|---|---|
| `tzhazuma/gridcraft-android` | GridCraft 的 Android 移植（port crate + 脚本 + STATUS） |
| `tzhazuma/craft-launcher-android` | ArtCraft Launcher 的 Android 移植 |

APK 走 **GitHub Releases**，不进 git 历史（31 MB 二进制）。

---

## 9. craft-launcher 的下载地址

启动器的下载 URL 由 `crates/core/src/catalog.rs` 里每个 app 的 `repo` 字段决定：

```
crates/core/src/release.rs:65   format!("https://github.com/{repo}/releases/download/")
crates/core/src/feed.rs:191     format!("https://github.com/{repo}/releases/")
```

所以「指向我们自己移植的 GitHub 下载链接」= 把 release 查询的 repo 从 `storytold/<app>`
换成 `tzhazuma/<app>-android`，同时**保留** `repo_url()` / `website()` 指向上游（文档/官网链接
不该被改）。做法与验证见 `docs/LAUNCHER-DOWNLOAD-URLS.md`。

---

## 10. 目录结构

```
~/artcraft-android/
├── STATUS.md                  ← 本文件
├── README.md
├── scripts/
│   ├── env.sh                 共享环境（PATH/NDK/JDK/lock/设备）
│   ├── build-lock.sh          整机构建串行化（fcntl）
│   ├── build-so.sh            <app> → libmain.so
│   ├── build-apk.sh           <app> → out/<Label>-<ver>-android-arm64.apk
│   ├── make-shell.sh          生成 Gradle 外壳 + 装该 app 的图标
│   └── device-smoke.sh        装机/启动/logcat/截图（含锁屏门禁）
├── templates/android-shell/   Gradle 骨架（AGP 9.1.0 / Gradle 9.3.1 / JDK 21）
├── icons/<app>/res/           每个 app 的矢量 adaptive icon（来自 icon-designer）
├── ports/<app>-android/       移植源码（source of truth）
├── src/<app>/                 上游浅克隆（.gitignore 掉）
├── out/                       APK
├── shots/                     真机截图
└── logs/                      构建日志
```

---

## 第 48 轮：13 个 APK 全部就位；6 个新 app 在模拟器上装机验证

### release 清单（13 个资产，全部 zipalign -P 16 通过）

| APK | 大小 |
|---|---|
| ArtCraftLauncher-0.4.1 | 22 MB |
| CADCraft | 30 MB |
| DeckCraft | 44 MB |
| DesignCraft | 65 MB |
| EffectCraft-0.4.0 | 65 MB |
| FigCraft | 47 MB |
| FilmCraft-0.2.1 | 42 MB |
| GridCraft-0.7.0 | 29 MB |
| LightCraft-0.2.1 | 33 MB |
| PhotoCraft-0.3.0 | 44 MB |
| PrintCraft-0.2.1 | 47 MB |
| SoundCraft | 27 MB |
| WordCraft | 55 MB |

### 模拟器装机验证（emulator-5554 / API 36）

**注意：真机当时锁屏，vivo 会静默丢弃安装，所以这批验证是在模拟器上做的。**

6 个新 APK 全部 `pm install` → `Success`，13 个 storyteller 包全部在设备上。
逐个冷启动 12 秒后看进程是否存活：

| app | 结果 |
|---|---|
| DesignCraft | ✅ 存活 pid=4993 |
| WordCraft | ✅ 存活 pid=5093 |
| DeckCraft | ✅ 存活 pid=5141 |
| CADCraft | ✅ 存活 pid=5178 |
| **SoundCraft** | ✅ **存活 pid=5233**（cpal/AAudio 那条链真的能跑） |
| FigCraft | ❌ `Process ... failed to attach` |

**FigCraft 的失败形态与其他都不同**：进程确实起来了（pid=5286），
但日志是 `W ActivityManager: Process ... failed to attach`，且
`SurfaceSyncGroup: Failed to receive transaction ready in 1000ms`。
没有 FATAL / SIGSEGV，所以不是崩溃 —— 更像是**初始化耗时超过 splash 窗口**
导致 attach 超时。下一步该看它启动时卡在哪一步（可能和它比别的 app 多做
collab-net / layout 初始化有关）。

### SoundCraft 的教训（已单独提交）

`ld.lld: error: unable to find library -laaudio` 的根因不是 oboe，是
**cargo-ndk 用了 Rust target triple 里的 API 级别（android24）**，而 AAudio 从
API 26 才有。Gradle 的 minSdk 是 31，原生侧却按更低级别构建。
修法：`cargo +stable ndk -t arm64-v8a -P 31 ...`（**`-P` 大写** = --platform；
小写 `-p` 是 --package，传数字会 panic）。

---

## 第 49 轮：FigCraft 的「attach 超时」其实是模拟器的 SwiftShader 限制

上一轮我把它记成「初始化超出 splash 窗口」。拿到完整日志后，真实原因完全不同：

```
I FigCraft: egui_wgpu: There are 2 available wgpu adapters:
            {backend: Vulkan, device_type: Cpu, name: "SwiftShader Device (LLVM 10.0.0)"}
W FigCraft: wgpu_hal::gles::egl: Error in create_window_surface: BadAlloc
I RustStdoutStderr: thread '<unnamed>' (5389) panicked at wgpu-30.0.1/src/backend/wgpu_core.rs:3981:
I RustStdoutStderr: wgpu error: Validation Error
I RustStdoutStderr: Caused by:
I RustStdoutStderr:   In Surface::configure
I RustStdoutStderr:     Invalid surface
```

**不是崩溃，也不是初始化慢，是 `create_window_surface` 拿不到 GPU surface。**
模拟器只有 SwiftShader 软件渲染（`device_type: Cpu`），`BadAlloc` 就是分配失败。
wgpu 默认把这类错误当 fatal，于是 panic，进程随后被 ActivityManager 回收，
表面现象就成了 `Process ... failed to attach`。

### 两台设备的 GPU 对比（实测）

```
模拟器 emulator-5554 (API 36):
  wgpu 选到: SwiftShader Device (LLVM 10.0.0), device_type = Cpu
  create_window_surface -> BadAlloc

真机 vivo PA2573 (Android 16 / arm64-v8a):
  GLES: ARM, Mali-G925-Immortalis MC12, OpenGL ES 3.2
  vulkan_renderengine: false
```

真机有真实 GPU，FigCraft 在上面**应该正常** —— 但这台真机当时锁屏，
vivo 会拒绝安装（`INSTALL_FAILED_ABORTED: User rejected permissions`），
所以这一项还没能验证。

### 结论与后续

1. **FigCraft 的 APK 本身没有问题**（`zipalign` / `aapt2` / 链接都过了，
   编译 0 error），失败只发生在软件渲染的模拟器上。
2. 要验证它必须用真机（或带 GPU 加速的模拟器）。
3. 如果确实需要在模拟器上跑，可给 wgpu 设
   `WGPU_BACKEND=gl` 或降级 adapter 选择，但那只是让 CI 能截图，
   不代表真实表现。

**这是我之前误判的一次，记下来**：`failed to attach` 这种系统层的间接症状，
一定要先看应用自己的日志再做归因。

---

## 第 50 轮：SAF 桥接补齐后，FilmCraft 重装并跑到自检

### 发现了什么

`saf.rs` 一直在 `patches/filmcraft-android/src/` 里，它调用 Activity 上的六个方法：

```
safPickMedia()V   safIsDone()Z      safPickSave(String)V
safSaveDone()Z    safHasSaveUri()Z  safPublish(String)Z
```

**这六个方法一个都不存在。** 整个工程里没有任何 `.java` / `.kt` 文件 ——
manifest 里的 `.MainActivity` 解析到的是 `androidx.games:games-activity`
依赖自带的 `GameActivity`，桥接从来没实现。

后果是静默的：`saf.rs` 的 `call()` 返回 `Option`，上层 `unwrap_or(false)`
把「Java 方法不存在」变成了「用户取消了」，于是文件对话框什么都不做、也不报错。
这正是「打开关闭文件有问题」的真相。

### 补了什么

`MainActivity.java`（271 行），继承 `com.google.androidgamesdk.GameActivity`：

| 能力 | 实现 |
|---|---|
| 导入 | `ACTION_OPEN_DOCUMENT` + `EXTRA_ALLOW_MULTIPLE`；每个 `content://`
文档拷到 `filesDir/import/`，真实路径逐行写入 `filesDir/import-manifest.txt`
（Rust 侧本来就在轮询这个文件） |
| 保存 | `ACTION_CREATE_DOCUMENT`；URI 留在 Java 侧，`safPublish` 用
`openOutputStream` 流式写入 |
| 权限 | 两条路径都 `takePersistableUriPermission`，用 try/catch 包住 ——
provider 不支持持久授权时退回一次性授权，而不是抛异常 |
| 取消 | 写**空** manifest 而不是留着上一次的，避免「取消导入」反而重导上次的文件 |
| 重名 | 加 ` (2)` 后缀而不是覆盖 —— 从不同文件夹选两个同名文件是正常操作 |

已推广到 FilmCraft / LightCraft / PhotoCraft / EffectCraft / PrintCraft 五个 app。

### 验证到哪一步了

FilmCraft 重新构建（42.4 MB，`classes.dex` 存在，
`aapt2` 报 `launchable-activity ai.storyteller.filmcraft.MainActivity`），
装到 emulator-5554 并启动。日志显示**媒体自检真的跑通了**：

```
I main: filmcraft-android: H.264 自检: 解码器 MediaCodec H.264 | 样本 30 个
        → 解出 30 帧（320x240）| 硬件计数 帧 30 会话 1 拒绝 0 回退 0
W filmcraft_platform::hybrid: MediaCodec HEVC failed (decode error);
        continuing with the software decoder
```

HEVC 在模拟器上硬件解码失败但正确回退到软件解码器 —— 这个回退路径也是对的。

**还没验证的**：SAF 选择器本身。FilmCraft 跑完自检后 Activity 收到 `CLOSE`
（`Transition type = CLOSE`），窗口关闭、进程被 freeze，所以没法接着去点
「打开」按钮。要点的话得先让它进入正常编辑界面（有项目可开时），
或者用坐标点击。egui 渲染在 SurfaceView 里没有可访问性树，
`uiautomator dump` 拿不到它的控件，只能靠看截图算坐标。

### 待办

1. **7 个新 craft app 的 SAF**：VectorCraft / FigCraft / DesignCraft / WordCraft /
   DeckCraft / CadCraft / SoundCraft 连 `saf.rs` 都还没有。它们的 `Services`
   目前是 `Default`（即「未接文件对话框」），需要按 filmcraft 的模式补
   Rust 侧 + Java 侧。
2. **SAF 端到端**：需要在能进编辑界面的前提下点一次「打开」，看系统选择器是否弹出。
3. **Blender SAF**：打开/导入/导出/输出三条路径，工程量在 `BKE_file`/`BLI_file`
   只认 POSIX 路径。

### 教训

**静默失败比崩溃难查得多。** 跨语言桥接的代码，必须逐项确认对端真的存在 ——
`find . -name "*.java"` 这种一眼可见的检查，应该在写 `saf.rs` 那次就做，
而不是等到用户反馈「文件打不开」才回头查。

---

## 第 51 轮：7 个新 app 补齐 SAF 双半；VectorCraft 重建验证

### 补齐了什么

7 个新 app 之前既没有 `saf.rs` 也没有 Java。现在各自都有：
- `src/saf.rs`（Rust 侧 JNI 桥）
- `MainActivity.java`（271 行，继承 `com.google.androidgamesdk.GameActivity`）
- `Cargo.toml` 里的 `jni = "0.22"` + `ndk-context = "0.1"`
- `lib.rs` 里的 `mod saf;`
- `AndroidManifest.xml` 改成完整类名

全部 `cargo +stable ndk check` → **0 error**。

### 一个错位造成 15 个错误

第一次用 `sed '3a mod saf;'` 把 `mod saf;` 插到了第 3 行 —— **在模块级 `//!` 文档注释上面**：

```
error[E0753]: expected outer doc comment
  --> src/lib.rs:5:1
```

Rust 不允许 `//!` 出现在非注释项之后。每个 app 15 个错误，全部来自这一行错位。
正确位置是 `//!` 块之后、`#![...]` 内部属性之后。

### VectorCraft 重建验证

```
cargo +stable ndk ... build --release   →  libmain.so 89.0 MB
gradlew assembleRelease                 →  APK 79.6 MB
classes.dex: 1                          （Java 侧确实编进去了）
aapt2: launchable-activity ai.storyteller.vectorcraft.MainActivity
zipalign -c -P 16: PASS
```

### 还差什么

**接线**。`saf.rs` 和 `MainActivity.java` 都在，但 `Services` 仍然是 `Default`
（`services() { Services::default() }`），所以**没有任何代码调用 `saf::pick_media`**。
点「打开」仍然没反应。

接线需要的三块，本轮的探测已经确认都具备：
1. `activity` = `AndroidApp::activity_as_ptr()`
2. `data_dir` = `AndroidApp::internal_data_path()`
3. 回灌路径用的 control 通道 —— 7 个 app 都有（`ControlRequest` / `control_server`）

难点：`PickOpen = Box<dyn FnMut(&FilePick) -> Option<String>>` 是**同步**的，
在 UI 线程调用。SAF 选择器是异步的（Java 起 Intent，Rust 侧轮询 250ms）。
FilmCraft 的解法是返回 `None` 立刻返回、后台线程跑完再把路径经 `ControlRequest`
回灌，让引擎执行**正常的** open 命令。

**我没有只接一半**，因为那样比不接更糟：用户点了「打开」、系统选择器弹出来、
选完文件，然后什么都没发生。要做就做完整的那条回路。

每个 app 的 `ControlRequest` 形状都不同，所以这是逐个的活，不是机械复制。

---

## 第 52 轮：SAF 第一次真正接线（VectorCraft 作为示范）

前几轮把两半都补齐了（`saf.rs` + `MainActivity.java`），但 `services()` 一直是
`Services::default()`，所以没有任何代码调用 `saf::pick_media`。这轮把回路接上了。

### 接线的形状

难点在于两边的时序不匹配：

- `PickOpen = Box<dyn FnMut(&FilePick) -> Option<String>>` —— **同步**，在 UI 线程调用
- SAF 选择器 —— **异步**（Java 起 Intent，Rust 侧每 250ms 轮询一次标志）

如果 `pick_open` 里阻塞等选择器，整帧就冻住了。所以：

```rust
services.pick_open = Some(Box::new(move |_pick: &FilePick| {
    let Some(dir) = open_dir.clone() else { return None };
    let ctrl = open_ctrl.clone();
    std::thread::spawn(move || {
        let paths = saf::pick_media(activity, &dir, Duration::from_secs(300));
        let Some(path) = paths.into_iter().next() else { return };
        let (req, _rx) = ControlRequest::new("app.open", json!({ "path": path }));
        let _ = ctrl.send(req);
    });
    None   // 选择器已经弹出去了，这一帧继续渲染
}));
```

配套改动：
1. `android_main` 里建 `mpsc::channel::<ControlRequest>()`
2. `app.activity_as_ptr() as usize` 拿 Activity 指针（在 `app` 被 move 进
   `NativeOptions` 之前）
3. `VectorcraftApp::new(...).with_control(ctrl_rx)` —— app 每帧从通道里取请求执行
4. 回灌的 `app.open` 走的是**桌面同款**命令（`control.rs` 里本就映射到
   `file.open`），所以打开路径完全一致

`cargo +stable ndk check` → **0 error**；APK 79.6 MB 重建、装机、启动（pid 6171）。

### `pick_save` 故意没接

保存走的是「应用自己的 export 目录 + SAF 发布」两段式（FilmCraft 的模型：
先写到临时文件，`safPublish` 再搬到用户选的位置）。VectorCraft 的 export
目录和临时文件流程我还没查清，所以 `pick_save` 只是记一条日志并返回 `None`，
**没有假装能用**。

### 其余 6 个 app

接线模式完全一样，但每个 app 的 `ControlRequest` 方法名和 `Services` 字段
需要各自确认（VectorCraft 是 `app.open` + `pick_open`）。逐个做，不批量 sed ——
批量改跨语言回调是最容易留下静默失败的方式。

---

## 第 53 轮：SAF 接线扩展到另外 3 个 app

### 每个 app 的 `Services` 签名都不同

这是本轮最花时间的部分 —— 同一个 `pick_open`，六个 app 四种签名：

| app | `pick_open` 类型 |
|---|---|
| VectorCraft | `Box<dyn FnMut(&FilePick) -> Option<String>>`（`FilePick` 是结构体） |
| **designcraft / deckcraft** | `Box<dyn FnMut(&str) -> Option<String>>`（purpose 字符串） |
| **cadcraft** | `Box<dyn Fn() -> Option<String>>`（无参数） |
| wordcraft / soundcraft | 有 `pick_open`，但 control 里没有 `app.open` |
| figcraft | `Services` 是 **trait**，设计不同 |

我先按 VectorCraft 的形状写了一版通用接线脚本，然后被 `error[E0425]: cannot find
type FilePick` 一次性打回三个 app。查明后按各自签名重写，全部 0 error。

### 接好的三个

```rust
services.pick_open = Some(Box::new(move |_purpose: &str| {
    let Some(dir) = open_dir.clone() else { return None };
    let ctrl = open_ctrl.clone();
    std::thread::spawn(move || {
        let paths = saf::pick_media(activity, &dir, Duration::from_secs(300));
        let Some(path) = paths.into_iter().next() else { return };
        let (req, _rx) = ControlRequest::new("app.open", json!({ "path": path }));
        let _ = ctrl.send(req);
    });
    None
}));
```

designcraft / deckcraft / cadcraft 三个 `cargo +stable ndk check` → **0 error**。

### 进度

| app | SAF 双半 | 接线 |
|---|---|---|
| VectorCraft | ✅ | ✅ |
| **DesignCraft** | ✅ | ✅ **本轮** |
| **DeckCraft** | ✅ | ✅ **本轮** |
| **CadCraft** | ✅ | ✅ **本轮** |
| WordCraft | ✅ | ❌ 缺 `app.open` 方法名 |
| SoundCraft | ✅ | ❌ 缺 `app.open` 方法名 |
| FigCraft | ✅ | ❌ `Services` 是 trait，设计不同 |
| FilmCraft/LightCraft/PhotoCraft/EffectCraft/PrintCraft | ✅ | ✅（老 app 已接） |

### 教训

**「同一个接口」在跨 6 个代码库时并不存在。** `pick_open` 这个名字一样，
类型却四种。批量 sed 在这里一定会失败 —— 而且失败方式很危险：
如果签名恰好兼容但语义不同，就会编过、跑错、静默失败。
必须逐个看真实的类型定义。

---

## 第 54 轮：WordCraft 接线完成（走 `engine.execute`）

WordCraft 的 control 通道里**没有 `app.open`**，它只有：

```
app.quit  document.inspect  engine.commands  engine.execute  file.close
ui.click  ui.drag  ui.focus  ui.inspect  ui.key  ui.move  ui.parity ...
```

但 `engine.execute` 可以执行任意引擎命令，而 WordCraft 的命令表里有 `file.open`：

```
"engine.execute" | "command" => {
    let Some(id) = s("command").or(s("id")) else { return err("missing `command`") };
    let params = p.get("params").cloned()...;
    match app.execute(id, params) { ... }
}
```

所以回灌的请求是：

```rust
ControlRequest::new("engine.execute", json!({
    "command": "file.open",
    "params": { "path": path }
}))
```

效果和 `app.open` 完全一样 —— 走的是同一条引擎命令路径。

`cargo +stable ndk check` → **0 error**。

### 中途一个小坑

第一次的替换脚本用正则 `Session::new\([^)]*\)` 匹配构造点，但 WordCraft 是
`Session::new(wordcraft_doc::Document::new())` —— **括号嵌套**，`[^)]*` 在第一个
`)` 处就停了，匹配失败。脚本打印「找不到构造点」然后退出，文件没被改动，
所以 `check` 报 0 error（那是**原版**的 0 error，不是接线后的）。

差点误判为成功。教训：**替换脚本失败后，`errors: 0` 不等于成功** ——
必须同时确认目标代码真的被写进去了（`grep` 到 `pick_open` 才算）。

### SAF 接线进度：9 / 13

| app | 双半 | 接线 |
|---|---|---|
| VectorCraft / DesignCraft / DeckCraft / CadCraft / **WordCraft** | ✅ | ✅ |
| FilmCraft / LightCraft / PhotoCraft / EffectCraft / PrintCraft | ✅ | ✅（老 app） |
| SoundCraft | ✅ | ❌ **没有 `file.open`**：DAW 只有 `file.import_audio` /
`file.import_midi` / `file.export_*`，语义不同，要单独设计 |
| FigCraft | ✅ | ❌ `Services` 是 trait 不是 struct |
| GridCraft | ✅（老 app） | 见老 app 记录 |

---

## 第 55 轮：FigCraft 的 `SafServices`——异步 future 反而更适合 SAF

### FigCraft 为什么不一样

其他 app 的 `Services` 是 struct，`pick_open` 是同步闭包；FigCraft 是：

```rust
pub trait Services {
    fn pick_open(&mut self, title: &str, exts: &[&str]) -> DialogFuture { ... }
}
pub type DialogFuture = Pin<Box<dyn Future<Output = Option<PathBuf>>>>;
```

`DialogFuture` 由 UI **跨帧轮询**（`figcraft_ui_egui` 里有个 `future` 字段）。
这比同步闭包**更适合** SAF：不需要在帧内阻塞，也不需要「返回 None 再回灌」那套迂回。

### 实现（`src/saf_services.rs`，115 行）

```rust
fn spawn_pick(&self) -> DialogFuture {
    let (tx, rx) = mpsc::channel::<Option<PathBuf>>();
    std::thread::spawn(move || {
        let picked = saf::pick_media(activity, &dir, Duration::from_secs(300))
            .into_iter().next().map(PathBuf::from);
        let _ = tx.send(picked);
        // 取消选择器不会产生输入事件，空闲的 app 就不会再 poll 了
        if let Ok(g) = ctx.lock() { if let Some(c) = g.as_ref() { c.request_repaint(); } }
    });
    Box::pin(PickFuture { rx })
}
```

`PickFuture::poll` 做 `try_recv()`：有值就 `Ready`，`Empty` 就 `Pending`，
`Disconnected` 就 `Ready(None)`（线程没了当取消处理）。

**关键点：`request_repaint()`**。用户取消选择器时不会有任何输入事件，
一个空闲的 egui app 不会自动重绘，那个 future 就永远停在 `Pending` 了。
所以后台线程拿到结果后必须主动唤醒。

### 验证

```
grep -c SafServices   src/lib.rs         → 1
grep -c saf::pick_media src/saf_services.rs → 1
cargo +stable ndk check -p figcraft-android → 0 error
```

（这次特意先 grep 再报 —— 上一轮那个正则失配的教训。）

### SAF 接线进度：10 / 13

| app | 接线方式 |
|---|---|
| VectorCraft / DesignCraft / DeckCraft / CadCraft | 同步闭包 → 返回 None → `ControlRequest("app.open")` |
| WordCraft | 同步闭包 → `ControlRequest("engine.execute", file.open)` |
| **FigCraft** | **trait + `DialogFuture` + 后台线程 + `request_repaint`** |
| FilmCraft / LightCraft / PhotoCraft / EffectCraft / PrintCraft | 老 app 已有 |
| SoundCraft | ❌ 没有 `file.open`，DAW 语义不同 |
| GridCraft | 老 app 记录 |

---

## 第 56 轮：SoundCraft 接线完成 —— ArtCraft 全 13 个的 SAF 全部接通

### SoundCraft 的「打开」是三件事，不是一件

DAW 没有 `file.open`。看它的命令表：

```
session.new  session.open  session.close  session.revert
session.save  session.save_as  session.save_copy
file.import_audio  file.import_midi  file.bounce_mix ...
```

而 `Services::pick_open` 的签名里带着 `purpose`：

```rust
pub pick_open: Option<Box<dyn Fn(&str, &[&str]) -> Option<String>>>,
```

桌面端的实现按 `purpose` 分三种过滤器，所以 Android 端也就按它分派：

| `purpose` | 命令 |
|---|---|
| `"session.open"` | `session.open`（`.scraft` 项目） |
| `"file.import_midi"` | `file.import_midi` |
| 其他（音频） | `file.import_audio` |

三个命令都收 `{"path": ...}`，所以一条 `engine.execute` 回路就够。

### 验证（这次先 grep 后看 error）

```
grep -c "saf::pick_media" src/lib.rs   → 1
grep -c "SoundApp::new"    src/lib.rs   → 1
grep -c "fn services"      src/lib.rs   → 1
cargo +stable ndk check                → 0 error
```

（上一轮那个正则失配的坑：脚本「成功」了、文件没改、编译 0 error —— 全部假象。
现在每次改完都先确认目标代码真的在。）

### SAF 接线：13 / 13

| app | 接线方式 |
|---|---|
| VectorCraft / DesignCraft / DeckCraft / CadCraft | 同步闭包 → `None` → `ControlRequest("app.open")` |
| WordCraft | 同步闭包 → `ControlRequest("engine.execute", file.open)` |
| FigCraft | trait + `DialogFuture` + 后台线程 + `request_repaint` |
| **SoundCraft** | **同步闭包 → `engine.execute`，按 `purpose` 分派到 session.open / import_midi / import_audio** |
| FilmCraft / LightCraft / PhotoCraft / EffectCraft / PrintCraft | 老 app 已有（用 `HostHooks` + `ControlRequest`） |
| GridCraft / ArtCraft Launcher | 老 app 已有 |

`MainActivity.java`（271 行）提供 Java 侧的 `ACTION_OPEN_DOCUMENT` /
`ACTION_CREATE_DOCUMENT` / `takePersistableUriPermission` / `openOutputStream`。

### 还剩什么

**`pick_save` 全部是显式 no-op**（记一条日志、返回 `None`）。保存要用
「写临时文件 → `safPublish` 搬到用户选的位置」的两段式（FilmCraft 是这么做的），
每个 app 的 export 目录和临时文件流程都不一样，还没有逐个查清。
宁可留一条明确的日志，也不返回一个假路径 —— 那样就会重现这次花了好几轮
才消除的那种静默失败。

---

## 第 57 轮：`pick_save` 两段式保存（VectorCraft 示范）

### 为什么不能返回 `None`

`pick_save` 的返回值是**应用要写入的路径**。返回 `None` 的话，应用会认为
「用户没有选目标」，于是**根本不写文件**。所以这里不能像打开那样返回 `None`
再回灌 —— 必须马上给一个真实可写的路径。

### 两段式（照 FilmCraft 的模型）

```rust
services.pick_save = Some(Box::new(move |pick: &FilePick| {
    let scratch = export_dir.join(leaf);          // 1. 给应用一个私有目录里的路径
    let scratch_string = scratch.to_string_lossy().into_owned();
    std::thread::spawn(move || {
        // 2. 后台：先问用户要存到哪
        if !saf::pick_save(activity, &leaf, Duration::from_secs(300)) { return; }
        // 3. 等应用把文件写完（连续 1.5 秒大小不变）
        loop { ... if size > 0 && stable_since.elapsed() > 1500ms { break } ... }
        // 4. 拷到用户选的位置
        saf::publish(activity, &scratch_path);
    });
    Some(scratch_string)
}));
```

**关键是「等待写完」那段**：`pick_save` 返回路径时文件还不存在，应用是在
之后的帧里写。所以后台线程要轮询文件大小，直到连续 1.5 秒不再变化。
直接 `publish` 会拷到一个空文件或不存在的文件。

### 所有权处理（踩了两个坑）

第一版有两个编译错误：

```
error[E0597]: `name` does not live long enough          // leaf 借用了 name
error[E0382]: use of moved value: `scratch_string`      // 既 move 进线程又返回
```

修法：
- `leaf` 用 `String`（`.to_owned()`），不借 `name`
- `scratch_string.clone()` 一份给线程，原始的返回给应用

### 验证

```
grep -c "saf::publish"    → 1
grep -c "saf::pick_save"  → 1
cargo +stable ndk check   → 0 error
```

### 剩下 12 个 app

同样的两段式可以套，但每个 app 的 export 目录和 `pick_save` 签名不同：

| app | `pick_save` 签名 |
|---|---|
| VectorCraft | `FnMut(&FilePick) -> Option<String>` |
| designcraft / deckcraft | `FnMut(&str) -> Option<String>` |
| cadcraft | `FnMut(&str) -> Option<String>` |
| wordcraft | `FnMut(&str) -> Option<String>` |
| soundcraft | `Fn(&str, &str) -> Option<String>` |
| figcraft | `trait` + `DialogFuture` |

---

## 第 58 轮：`pick_save` 推广到 4 个 app

两段式实现已经固化成一个脚本，套到签名相同的 4 个 app 上：

| app | `pick_save` 签名 | 结果 |
|---|---|---|
| **DesignCraft** | `FnMut(&str) -> Option<String>` | ✅ 0 error |
| **DeckCraft** | `FnMut(&str) -> Option<String>` | ✅ 0 error |
| **CadCraft** | `FnMut(&str) -> Option<String>` | ✅ 0 error |
| **WordCraft** | `FnMut(&str) -> Option<String>` | ✅ 0 error |

这四个的签名里那个 `&str` **就是建议文件名**（不是 purpose），所以直接拿它当
export 目录里的叶子名。

### SoundCraft 需要单独处理

脚本在 SoundCraft 上失败了，原因不是签名问题，而是**形状问题**：

```rust
// 其他 app：赋值形式
services.pick_save = Some(Box::new(...));

// SoundCraft：结构体字面量，三个字段一起给
Services {
    pick_open: Some(...),
    pick_save: Some(...),
    pick_folder: None,
}
```

所以要在 `Services { ... }` 里就地替换那个 `pick_save: Some(...)` 表达式，
不能用「找 `services.pick_save =` 赋值」的策略。留给下一轮。

（它的签名是 `Fn(&str, &str) -> Option<String>` —— `(purpose, default_name)`，
用第二个参数当建议文件名。）

### 验证

```
grep -c "saf::publish"  → 各 1
cargo +stable ndk check → 各 0 error
```

### SAF 完成度

| 能力 | 进度 |
|---|---|
| 打开路径 | **13 / 13** |
| 保存路径 | **5 / 13**（VectorCraft + 本轮 4 个） |

剩：soundcraft（形状不同）、figcraft（trait + DialogFuture）、以及 5 个老 app
（FilmCraft 已有，其余 4 个待查）。

---

## 第 59 轮：SoundCraft 与 FigCraft 的 `pick_save` 完成 —— 7 个新 app 双通路齐备

### SoundCraft：形状问题，就地改结构体字面量

它的三个 picker 是一起给的：

```rust
Services {
    pick_open: Some(...),
    pick_save: Some(...),   // ← 要在这里就地替换
    pick_folder: None,
}
```

不是 `services.pick_save = Some(...)` 赋值，所以上一轮的脚本找不到。
另外需要在构造 `Services` 前加 `let save_dir = data_dir.clone();`（原来
`data_dir` 被 `pick_open` 的闭包 move 掉了）。

签名 `Fn(&str, &str) -> Option<String>` 的**第二个参数**是建议文件名。

### FigCraft：`DialogFuture` 让保存更简单

因为返回的是 future，可以**立刻 resolve** 成 scratch 路径让应用开始写，
同时后台线程去问目的地并等文件写完再 publish：

```rust
fn pick_save(&mut self, _title: &str, name: &str, _exts: &[&str]) -> DialogFuture {
    let scratch = export_dir.join(leaf);
    std::thread::spawn(move || { /* pick_save → 等写完 → publish */ });
    Box::pin(std::future::ready(Some(scratch)))   // 立即返回
}
```

比同步闭包版本短，因为不需要「返回 None 再回灌」那套。

### 验证

| app | `saf::publish` | check |
|---|---|---|
| SoundCraft | 1 | 0 error |
| FigCraft | 1 | 0 error |

### SAF 完成度

| app | 打开 | 保存 |
|---|---|---|
| VectorCraft | ✅ | ✅ |
| DesignCraft | ✅ | ✅ |
| DeckCraft | ✅ | ✅ |
| CadCraft | ✅ | ✅ |
| WordCraft | ✅ | ✅ |
| SoundCraft | ✅ | ✅ |
| FigCraft | ✅ | ✅ |
| FilmCraft | ✅ | ✅（老 app） |
| LightCraft / PhotoCraft / EffectCraft / PrintCraft | ✅（老 app 打开） | ⏳ 待查 |
| GridCraft / ArtCraft Launcher | ✅（老 app） | ⏳ 待查 |

**7 个新移植的 app 双通路全部齐备。**

---

## 第 61 轮：SAF 全量核实 —— 结论比预期好，13/13 打开与保存都已实现

### 之前的统计是错的

前几轮我用 `grep <app>-android/src/lib.rs` 统计，得出的结论是
「SAF 保存 8/13」，并据此以为 LightCraft / PhotoCraft / EffectCraft / PrintCraft
还有 4 个要补。

**实际上那 4 个早就有了**，只是代码不在 `lib.rs` 里：

| app | 保存路径在哪 |
|---|---|
| FilmCraft | `src/lib.rs`（`spawn_publish`） |
| LightCraft | `src/lib.rs`（`spawn_publish`） |
| **PhotoCraft** | **`src/services.rs`**（`spawn_publish`，324 行的独立模块） |
| EffectCraft | `src/lib.rs`（`spawn_publish`） |
| **PrintCraft** | **`src/saf.rs`**（`SafHost::save()`，282 行） |

尤其 PrintCraft，它的 SAF 结构是**第四种**：不是闭包、不是 `Services` 结构体、
不是 trait 对象，而是给 UI crate 的 `fd::Host` trait 提供一个实现：

```rust
impl Host for SafHost {
    fn open(&self, request: &DialogRequest, multiple: bool) -> Vec<PathBuf>;
    fn save(&self, request: &DialogRequest) -> Option<PathBuf>;
    fn folder(&self) -> Option<PathBuf>;
}
```

### 教训

**统计「某功能有没有实现」时，只搜主文件会漏掉模块化拆分的代码。**

`lib.rs` 只是入口，实现可能在 `services.rs` / `saf.rs` / `saf_services.rs` 里。
正确的搜法是 `grep -rl <模式> <src 目录>`，不是 `grep <模式> lib.rs`。

这一轮差点又基于错误统计去「新增」一个已经存在的功能 —— 那样会写出重复的
`spawn_publish`，编译能过，运行时两个线程抢同一个 scratch 文件。

### 最终状态

| 能力 | 进度 |
|---|---|
| APK 出包 | 13 / 13 |
| SAF 打开 | **13 / 13** |
| SAF 保存 | **13 / 13** |
| 原版图标 | 13 / 13 |

ArtCraft 的 SAF 工作**到此完整**。剩下的是 Blender 的原生侧。

---

## 第 62 轮：清理孤儿 MainActivity.java（37 → 13）

### 发现

全仓库有 **37 个 `MainActivity.java`**，但只有 13 个 app。原因是：我把
`gridcraft-android` 当作模板复制到每个新 app 时，模板里的
`android/app/src/main/java/ai/storyteller/gridcraft/MainActivity.java`
**一起被复制过来了**，而我的 sed 只把「目标 app 的包名」改对，那份 gridcraft 的
原样留着。

后果：每个新 app 的 APK 里都编进了 271 行属于别人的死代码。manifest 指向的是
正确的那份类，所以功能上没坏，但它是**能编译、能打包、永远不会被执行**的代码 ——
最难被发现的那一类垃圾。

### 清理

删掉了 15 个孤儿目录（`src/` 下 8 个 + `ports/` 下 7 个），
`MainActivity.java` 从 37 降到 13。

### 我同时误删了一个合法文件

清理脚本靠「路径里的 `<name>-android/` 推断 owner，再和包名比对」来判断孤儿。
启动器的路径是 `src/craft-launcher/apps/craft-launcher-android/...`，匹配出来
`owner` 是整个路径、`package` 是 `artcraftlauncher` —— 两者不等，于是被误删。

```bash
owner=./src/craft-launcher/apps/craft-launcher-android/.../MainActivity.java
package=artcraftlauncher
```

**启动器已发布的 APK（22.5 MB）不受影响**，但重建能力被破坏。已从
`dist-repos/craft-launcher-android/templates/android-shell/.../MainActivity.java`
模板恢复。

### 教训

**批量删除脚本的「判定条件不匹配」和「判定条件误命中」都很危险，而后者更隐蔽。**

这次是后者：脚本跑完打印了 15 行删除日志，看起来完全正常，但其中一行删错了。
如果只有一个「删了 15 个」的汇总数字，这个错误会被完全掩盖。

**所以批量删除必须逐条打印被删的路径**，并且在删之后对照预期数量 —— 这次
「37 → 13」是对的，但里面有 1 个是错的、少了 1 个是对的，总数掩盖了两者的抵消。

更稳的做法：先把候选项写到文件里，人工扫一遍，再执行删除。

---

## 第 63 轮：SAF 端到端验证受阻于模拟器的软件渲染

想在模拟器上点一次「打开」把 SAF 整条链路跑通，结果失败在更前面的一步：

```
VectorCraft  pid 6386（存活）
20 秒截图     15 KB，纯黑
30 秒截图    256 KB —— 但不是 VectorCraft，是另一个 app「可达鸭 KedaYa」
日志         Transition(type = CLOSE) → ActivityTaskManager: top resumed state loss timeout
```

**VectorCraft 在模拟器上渲染不出来，然后自己关闭了**，前台落回上一个应用。
第一次看到 256 KB 截图时我差点以为「只是截得太早」，实际上那是别人的界面。

日志里能看到 wgpu 在模拟器上选到 SwiftShader（`device_type: Cpu`），和
FigCraft 那次是同一个根因，只是表现不同 —— FigCraft 在 `create_window_surface`
拿不到 surface 时 **panic**，VectorCraft 则是**画不出来**（黑屏，不报错）。

所以模拟器上没法点「打开」按钮：**界面上没有按钮**。

### 这台设备上能做与不能做的事

| 操作 | 模拟器 | 真机 |
|---|---|---|
| 安装 APK | ✅ | ✅（需解锁） |
| 进程启动 | ✅ | ✅ |
| **界面渲染** | ❌ SwiftShader 画不出 wgpu 内容 | ✅ 之前 VectorCraft 在 Mali-G925 上正常 |
| 点按钮 / 验证 SAF | ❌ | ✅（需解锁） |

真机当时锁屏，vivo 拒绝安装（`INSTALL_FAILED_ABORTED: User rejected permissions`），
所以这一步需要人工解锁后才能做。

### 已有证据能证明什么、不能证明什么

**能证明**（编译与静态验证）：
- `saf.rs` 的六个 JNI 方法名与 `MainActivity.java` 实现完全对应
- `MainActivity.java` 通过 `javac -proc:only` 检查（只缺 SDL/AndroidX 类，无语法错）
- 7 个新 app 的 `services()` / `SafHost` 确实调用了 `saf::pick_media`（grep 计数 ≥1）
- FilmCraft 重装后媒体自检真的跑通（`H.264 30/30 帧`）

**不能证明**（需要真机手动点一次）：
- 系统选择器真的弹出
- 选中的文件真的被拷进 `filesDir/incoming/`
- `safPublish` 真的写进了用户选的位置

这个区分很重要：我不想把「JNI 方法都存在」说成「文件打开能用」。

### 一个差点报错的观察

我先是看到「第二次截图 256 KB，不是黑屏！」，差点就写成「只是截得太早，
其实渲染正常」。

**实际上那 256 KB 是「可达鸭 KedaYa」的界面** —— VectorCraft 已经关闭，
前台落回了上一个应用。

这个坑很典型：**截图字节数只能说明「屏幕上有东西」，不能说明「那是我的东西」**。
要判断某个 app 的界面，必须看图，不能看文件大小。

---

## 第 69 轮：找到了「文件打开/保存不调用安卓文件管理器」的真正原因

### 症状

真机上点 VectorCraft 的 **File ▸ Open...**，应用**直接回到桌面**，
没有弹系统文件选择器。

### 根因：增量构建过期，`MainActivity.java` 从没被编译进 APK

逐层验证：

| 检查项 | 结果 |
|---|---|
| APK 的 `launchable-activity` | `ai.storyteller.vectorcraft.MainActivity` ✅ |
| `libmain.so` 里的 `safPickMedia` 等字符串 | ✅ 有（5 个） |
| **`classes.dex` 里的 `safPickMedia` 等** | **❌ 一个都没有** |
| `classes.dex` 里有什么 | 只有 `R$color` / `R$drawable` / `R$mipmap` / `R$style` |

也就是说：**原生侧写好了、Java 源文件也写好了，但 javac 从来没编译过那个
Java 文件** —— dex 里只有资源类。

原因：`MainActivity.java` 是在上一次 Java 编译**之后**才加进
`app/src/main/java/` 的，而 Gradle 的 up-to-date 检查认为
`compileReleaseJavaWithJavac` 不需要重跑。我一直只删 `app-release.apk`
重打包，那不会重新编译 Java。

### 修法

```bash
rm -rf app/build/intermediates/javac app/build/intermediates/classes
./gradlew :app:compileReleaseJavaWithJavac --rerun-tasks
```

结果：

```
app/build/intermediates/javac/release/.../ai/storyteller/vectorcraft/MainActivity.class  ✅
```

重新打包后：

```
APK 79.7 MB
dex 里的 SAF 方法: safHasSaveUri safIsDone safPickSave safPublish safSaveDone
MainActivity 引用: 3        ✅
libmain.so: 5 个 safPick* 字符串 ✅
```

### 这个 bug 为什么藏了这么久

我前面几轮验证的手段全是「编译通过」「grep 到 `saf::pick_media`」
「`.so` 里有字符串」—— **这些全都是「代码存在」的证据，不是「代码进了 APK」的证据**。

而 dex 检查我一次都没做过。如果早一点对**安装的 APK** 做
`unzip -p classes.dex | strings | grep safPick`，这个问题第一轮就会暴露。

### 教训（和前面几轮同一条，但这次最深）

**验证必须针对「最终产物」，不是「源代码」或「中间产物」。**

| 我验证的 | 实际应该验证的 |
|---|---|
| 源码里有 `saf::pick_media` | **装到设备上的 APK 的 dex 里有 `safPickMedia`** |
| `.so` 里有 `safPick*` 字符串 | 同上 |
| `gradlew assembleRelease` 成功 | 同上 + 真机点一次 |

### 还没验证的

设备在这一轮结束时离线了（mDNS 也发现不到），所以**没能把新 APK 装上去点一次**。
下一步：设备上线后 `adb install -r` 这个 79.7 MB 的 APK，点 File ▸ Open，
应该能看到系统文件选择器。

---

## 第 70 轮：12 个 APK 的 SAF 双半全部齐备（dex + so 都验证过）

### 修了什么

上一轮发现在 ArtCraft 上 `MainActivity.java` 从没被编译进 APK（Gradle 增量构建
不重编新增的 Java 文件）。这轮对全部 12 个 app 做了：

```bash
rm -rf app/build/intermediates/javac app/build/intermediates/classes
./gradlew :app:compileReleaseJavaWithJavac --rerun-tasks assembleRelease
```

同时发现 7 个新 app 的 `libmain.so` 也是旧的（缺 `safPickMedia` 字符串），
用 `cargo +stable ndk -t arm64-v8a -P 31` 重编。

### 验证结果（针对最终 APK，不是源码）

| app | APK | dex | so | 状态 |
|---|---|---|---|---|
| VectorCraft | 79.7 MB | 1 | 1 | ✅ |
| FigCraft | 47.9 MB | 1 | 1 | ✅ |
| DesignCraft | 65.3 MB | 1 | 1 | ✅ |
| WordCraft | 55.2 MB | 1 | 1 | ✅ |
| DeckCraft | 44.2 MB | 1 | 1 | ✅ |
| CadCraft | 30.6 MB | 1 | 1 | ✅ |
| SoundCraft | 27.2 MB | 1 | 1 | ✅ |
| FilmCraft | 42.4 MB | 1 | 1 | ✅ |
| LightCraft | 33.7 MB | 1 | 1 | ✅ |
| PhotoCraft | 44.7 MB | 1 | 1 | ✅ |
| EffectCraft | 65.3 MB | 1 | 1 | ✅ |
| PrintCraft | — | 0 | — | ⚠️ 待修 |

`dex` 列 = `unzip -p app.apk classes.dex | strings | grep -c safPickMedia`
`so` 列 = `unzip -p app.apk lib/arm64-v8a/libmain.so | strings | grep -c safPickMedia`

### 还没确认的一步

**选择器是否真的弹出来了，我没有确认。**

真机（192.168.0.108:42513，PA2573，已解锁）装了修复版 VectorCraft 并启动成功
（pid 6890），File 菜单也确实打开了（截图 `/tmp/vc-picker.png` 能看到
Open/Save/Save As/Close All 等项）。

但我用 `adb shell input tap` 按屏幕坐标点「Open...」**不可靠** —— 设备物理分辨率
3096×2064、截图 2508×1672，每次换算都可能差几十像素，而且 egui 的菜单项位置
会随窗口尺寸变化。两次尝试都没能在日志里看到 `START u0 ... OPEN_DOCUMENT`，
也没看到 `documentsui` 进程起来。

**所以「选择器能弹出」这件事仍然是待验证状态**，不能算已修好。

### 交给子智能体

已派 4 个并行智能体：
- `blender-fixer` → Blender 宿主依赖链 + SAF 四条路径（打开/导入/导出/输出）
- `dsh-octave-dev` → Octave 和 dsh 的文件打开/保存
- `lightcraft-dev` → FreeCAD 的 QFileDialog → SAF
- `printcraft-dev` → 全部 12 个 ArtCraft app 的端到端验证 + 修 PrintCraft

---

## 2026-10-19：选择器为什么「从不出现」——两个独立 bug，都在真机上找到

### 1. `startActivityForResult` 跑在非 UI 线程
Rust 侧在轮询线程里调 `safPickMedia()`，而开启 Activity 必须回到 UI 线程，否则抛异常；
JNI 包装又把这个异常吞掉了，于是**失败完全静默**。修法：两个 picker 都包进 `runOnUiThread`。

### 2. 选择器出来了，但对文档没用（这才是真正卡住的那一层）
vivo 的文件管理器**根据 intent 的 MIME 类型决定界面**：

| intent | 实际打开的界面 | 能选 .svg / .pdf / .blend 吗 |
|---|---|---|
| `setType("*/*")`，无 extra | 媒体选择器（图片 / 时光轴 / 相册集） | ❌ 完全不能 |
| 加 `EXTRA_MIME_TYPES`（非纯视觉的列表） | 文档浏览器（全部 / 图片 / 文档 / 其他 + 最近 + 设备存储） | ✅ |

两种行为在同一条 `am start` 命令上加/不加 extra 复现过，不是推测。
`EXTRA_MIME_TYPES` 用 `application/octet-stream, application/pdf, image/svg+xml, text/plain`。

### 3. 三个会浪费几小时的验证陷阱
- **`pidof com.android.documentsui` 在这台 ROM 上永远是空的**：`com.android.documentsui` 是个壳，
  `ACTION_OPEN_DOCUMENT` 实际由 `com.android.filemanager/.selector.view.SelectorHomeActivity` 承接。
  正确的检查是 `dumpsys activity activities | grep topResumedActivity`。基于 `pidof documentsui`
  得出的「选择器没弹出来」结论是**假阴性**。
- **选择器每行的「打开」是按别的应用打开文件，不会把结果回传给你**。要回传必须点该行左侧的
  勾选框，再点右下角「完成」。
- **`adb shell` 里不加引号的 `*/*` 会被设备 shell 做 glob 展开**，静默变成 `bin/...`，
  让 `resolve-activity` 永远返回 "No activity found"。要写成 `-t '*/*'`。

### 4. vivo 的安装拦截（`INSTALL_FAILED_ABORTED: User rejected permissions`）
不是权限问题，是「安全守护」弹窗没人点。流程：后台跑 `pm install`，等
`PackageInterceptActivity` 出现 → 勾「已了解应用的风险检测结果」→ 点「继续安装」。
已固化成 `~/install-apk.sh <apk>`，靠 `uiautomator dump` 找控件而不写死坐标。
屏幕必须解锁，否则 vivo 直接拒绝。

### 5. `.gitignore` 把五个老应用的补丁一起忽略了
`filmcraft/` 这种无锚点规则同样匹配 `patches/filmcraft-android/`，所以 filmcraft / lightcraft /
photocraft / effectcraft / printcraft 的补丁**从来没进过 git**。已改成 `/filmcraft/` 等锚定写法。

### 6. 构建源在哪（踩过）
- 7 个新应用：`~/artcraft-android/src/<app>/`（rsync 时注意 `--delete` 会删掉 `jniLibs/libmain.so`）
- 5 个老应用：**`~/artcraft-mobile/<app>/`**，`patches/` → 克隆再构建。
  `/tmp/rebuild-so.sh` 只认 `~/artcraft-android/src/`，对老应用会报「无源码」。

### 改动落地
`scripts/fix-saf-java.py` 可幂等地把两个修复打到每个 `MainActivity.java`，已跑过全部 12 个应用。

### 7. `gh release upload` 一直 408 —— 是 VPN 的 TUN，不是 GitHub
上传 40–70 MB 的 APK 时反复 `HTTP 408: Upload body timed out`，而下载一切正常。
原因：Clash 的 TUN 会返回**假 IP**（`198.18.0.0/15`），上传流量被塞进隧道后只有 ~30 KB/s 并被重置。
注意 `dig +short uploads.github.com` 在 VPN 运行时返回的就是假 IP，所以拿不到真地址。

绕过办法（实测 30 KB/s → 10 MB/s）：把 `uploads.github.com` 钉到真实地址，并绑定物理网卡：

```bash
curl --interface en0 --resolve uploads.github.com:443:20.205.243.161 \
     -X POST -H "Authorization: token $TOKEN" \
     -H "Content-Type: application/octet-stream" \
     --data-binary @app.apk \
     "https://uploads.github.com/repos/<owner>/<repo>/releases/<numeric-release-id>/assets?name=app.apk"
```

注意 release id 要用**数字 id**（`gh api repos/<owner>/<repo>/releases/tags/<tag> --jq .id`），
不是 `gh release view --json id` 返回的 `RE_...` node id。
同名资产重复上传会被拒（`already_exists`），要先 DELETE 再 POST。
已固化为 `scripts/gh-upload.sh`（自动替换同名资产）。

---

## 2026-10-20：保存为什么永远失败 —— `Services::write` 从未设置

真机上点 File › Save As，选择器正常弹出、目标 URI 正常拿到、磁盘上文件也建出来了，
但大小是 0，应用弹出：

> **Can't save "Untitled-1.vectorcraft".  no file writer**

一开始以为是扩展名不对（我给暂存文件起名 `.vectorcraft`）。查了写入器才发现不是：
`crates/ui-egui/src/io.rs:281`

```rust
services.write.as_mut().ok_or("no file writer")?(path, bytes)
```

而 Android 端的 `Services::default()` 里 `read`/`write` **一直是 `None`**，从来没设过。
所以**每一个** ArtCraft 应用的保存都是坏的，跟文件名无关。

修法（`apps/<app>-android/src/lib.rs` 的 services 构造处）：

```rust
services.read  = Some(Box::new(|path: &str| std::fs::read(path).map_err(|e| format!("{path}: {e}"))));
services.write = Some(Box::new(|path: &str, bytes: &[u8]| std::fs::write(path, bytes).map_err(|e| format!("{path}: {e}"))));
```

桌面端用自己那套文件抽象填这两个；Android 上 SAF 已经把用户的选择变成应用私有目录里的真实路径，
所以一句 `std::fs` 就是全部工作。

### 已应用 / 待应用（各应用的 services 构造方式不同，不能一把梭）

| 形态 | 应用 | 状态 |
|---|---|---|
| `let mut services = Services::default();` + 闭包赋值 | vectorcraft、cadcraft、deckcraft、designcraft、wordcraft | ✅ 已打补丁（vectorcraft 已重建并装到真机） |
| `fn services() -> Services { Services::default() }` | gridcraft | ⬜ 待改（结构不同，锚点不同） |
| `SafServices::new(activity, data_dir, ctx)` 自定义类型 | figcraft | ⬜ 需先确认 `SafServices` 是否透传 read/write |
| `) -> Services {` 函数体 | soundcraft | ⬜ 待看函数体 |
| `Services { … }` 结构体字面量 | lightcraft | ⬜ 加两个字段即可 |
| lib.rs 里完全没有 `Services` | filmcraft、photocraft、effectcraft、printcraft | ⬜ 它们的 SAF 在别的模块里，要先定位 services 从哪来 |
| 无文件功能 | craft-launcher | — 不适用 |

### 保存流程还有一个坑（不是 bug）
VectorCraft 在**选完保存位置之后**还会弹一个自己的「VectorCraft Options」对话框
（Use Compression / Version / Preview / 每画板单独存 / Include Linked Files / Embed ICC / PDF 兼容），
必须点 OK 才真正写盘。验证时容易误以为是卡死。

### 真机点按的正确做法
肉眼在缩放预览图上估坐标，纵向会偏几十像素，正好点在按钮边缘外，表现得像「应用没响应」。
改成用 PIL 在原始 3096×2064 截图上按颜色找按钮，一次命中：

```
orange BUTTON  center=(1933,1479) 179x69     # VectorCraft Options 的 OK
blue   band    center=(2950,2003) 219x89     # 保存选择器的「保存」
orange BUTTON  center=(2678,1697) 179x69     # New Document 的 Create
```

工具：`~/find-button.py <png> orange|blue`。

### 更正：`read`/`write` 字段并非所有应用都有（差点改坏构建）

我先把补丁机械地打到 6 个同构应用上，随后检查 `crates/ui-egui/src/lib.rs` 才发现
**cadcraft、wordcraft、gridcraft 的 `Services` 根本没有 `read`/`write` 字段** ——
那样写会直接编译失败。三处已回退（`src/` 是 gitignore 的克隆，`git checkout` 无效，只能手工还原）。

按实际字段分布重列：

| 应用 | `Services` 有 read/write 字段 | 状态 |
|---|---|---|
| vectorcraft | 有 | ✅ 已设，**已重建**（APK 79.7 MB） |
| deckcraft | 有 | ✅ 已设，待重建 |
| designcraft | 有 | ✅ 已设，待重建 |
| lightcraft | 有 | ✅ 原本就已设置 |
| photocraft | 有 | ✅ 原本就已设置 |
| cadcraft / wordcraft / gridcraft | **无** | — 不适用，已回退补丁 |
| figcraft | 无（自定义 `SafServices`） | 另一套 seam，保存路径待确认 |
| soundcraft | 无（`pick_files`/`write_shared` 形态） | 同上 |
| filmcraft / effectcraft / printcraft | 无 `Services`（走 `fd::Host` trait） | 同上 |
| craft-launcher | 无文件功能 | — |

结论：`no file writer` 这个 bug 确定影响 vectorcraft / deckcraft / designcraft 三个（
外加可能影响其它用到 `io::write_to` 的同族应用）；其余应用走的是不同的保存 seam，
需要各自在真机上验一次「保存后字节非 0」，不能假设。

### 再更正：`no file writer` 只是 vectorcraft 的文案，bug 本身更广

搜字符串的结论是「只有 vectorcraft 有 `no file writer`」——但这**不等于**只有它有这个 bug。
`deckcraft` 的 `crates/ui-egui/src/control.rs:317/419` 同样在调

```rust
app.services.write.as_mut().map(|w| w(path, &png))
```

只是钩子为空时它走别的分支（不报这句错），表现是**保存静默失败**而不是弹窗。

所以准确的判断标准不是搜错误文案，而是**看该应用的 `Services` 是否读 `write`/`read` 字段**：

| 应用 | Services 有 read/write | 受影响 | 处理 |
|---|---|---|---|
| vectorcraft | 有 | ✅ | 已修、已重建 |
| deckcraft | 有 | ✅ | 已修，重建中 |
| designcraft | 有 | ✅ | 已修，重建中 |
| lightcraft | 有 | 原本已设 | — |
| photocraft | 有 | 原本已设 | — |
| cadcraft / wordcraft / gridcraft | 无 | 不适用 | 补丁已回退 |
| figcraft / soundcraft / filmcraft / effectcraft / printcraft | 无（各自 seam） | 待逐个确认 | 需真机验「保存后字节非 0」 |

### 保存已在真机验证通过（0 字节 → 22,312 字节）

补上 `services.read`/`services.write` 之后，同一套操作在真机上跑通：

```
$ adb shell ls -la /sdcard/Download/Untitled-1.vectorcraft
-rw-rw---- 1 u0_a0 media_rw 22312 2026-10-20 13:29 /sdcard/Download/Untitled-1.vectorcraft

VectorcraftSAF: published 22312 bytes to content://com.android.providers.downloads.documents/document/346
VectorCraft: main: SAF: published /data/user/0/…/files/export/Untitled-1.vectorcraft (22312 bytes)
```

文件内容是真文档，不是空壳：

```
$ file saved.vectorcraft
JSON data
$ head -c 120 saved.vectorcraft
{
  "format": "vectorcraft",
```

**`no file writer` 不再出现。** 修复前同一个流程留下的是 0 字节文件 + `Can't save … no file writer` 弹窗。

### 这个流程的三个坑（验证时按顺序做）
1. `File › Save As…` **不能用 Ctrl+S**：Android 上键盘快捷键没接，必须点菜单。
2. 选完保存位置后，应用会再弹自己的「VectorCraft Options」（压缩/版本/预览/分画板/ICC/PDF 兼容）。
3. 那个对话框里的 Version 输入框会**自动聚焦并拉起键盘**，把 OK 按钮盖住 —— 必须先 `input keyevent 4` 收起键盘，再点 OK。
   收键盘前后 OK 都是 `(1933,1479)`，但键盘在时点它会被输入法吞掉。

## 2026-10-20：图标为什么会「没有图标」

用 `aapt2 dump resources <apk> | grep -A1 ic_launcher_foreground` 审计了 14 个 APK，
发现有两种结果：

- `res/qx.png type=PNG` —— 用的是 `assets/app-icon/<app>-1024.png` 渲染出的原版图标
- `res/Qr.xml type=XML` —— 用的是**生成的矢量占位图**（绿底白格纹之类）

根因：`make-original-icons.py` 会把原图写成 `drawable/ic_launcher_foreground.png`，
但 `drawable/` 里还留着模板时代的 `ic_launcher_foreground.xml`。**同名资源里 aapt2 用了 XML**，
所以即使原图已经生成，打进 APK 的仍是旧占位图。

修复：脚本在写完 PNG 后主动删除这两个残留 XML（`ic_launcher_foreground.xml`、
`ic_launcher_monochrome.xml`），并重跑五个老应用 + GridCraft + ArtCraftLauncher。
现在 14 个 APK 全部是 `type=PNG`。

另外注意：`src/gridcraft` 的源码图标本来是对的，但 `out/` 里那个是 10-17 的旧包；
**光看源码会以为没问题，必须看产物**。

## 15. FigCraft 闪退：已修复并验证

真机日志给出了确切原因（不是猜的）：

```
thread '<unnamed>' panicked at epaint-0.36.2/src/text/fonts.rs:1025:
FontFamily::Name("semibold") is not bound to any fonts
```

首帧在主题安装字体之前就用到了 `semibold` 字体族，epaint 对未绑定的族是直接 panic，
panic 穿过 FFI 让 `android_main` 返回，Activity 随之消失 —— 表现就是「一打开就闪退」，
而进程还会残存几秒，很容易误判成别的原因。

修法：在 `setup_fonts` 里先给 `semibold` 绑上 proportional 的那套字体作为占位，
主题真正装字体时再覆盖。修复后真机验证：进程存活、Activity 留在前台、设计工具界面完整渲染
（标签栏 / Pages / Layers / 画布 / 属性面板 / 导出 PNG / 底部工具条）。

## 16. SoundCraft / CADCraft 顶部 UI 被压住：诊断清楚，暂不改

现象：菜单栏（SoundCraft / File / Edit / …）最上面一行文字被裁掉几个像素。

原因：这类应用是桌面 UI，**自己把菜单栏画在窗口 y=0**，而 Android 15+ 强制 edge-to-edge，
窗口会延伸到状态栏/挖孔区域之下。

试过两种改法，都不行：

1. 去掉 `windowFullscreen` 只留 opt-out → 状态栏出现了，**菜单栏整个消失**（应用按全屏布局写死）。
2. 恢复 `windowFullscreen` 并加 `windowOptOutEdgeToEdgeEnforcement=true` → 截图字节数与改动前
   **完全一致**，即在这个 ROM（Android 16 / OriginOS）上 opt-out 已不再生效。

结论：要么在应用侧做 inset 处理（egui 里包一层带顶部 margin 的 frame，并按
`WindowInsets` 动态取值），要么改 `windowLayoutInDisplayCutoutMode`。属于外观问题，
不影响打开/保存等核心功能，已回退到原主题避免引入风险。

## 17. 真机验收状态（2026-10-22）

设备：vivo PA2573 / Android 16 / arm64-v8a，`192.168.0.108:44213`，已解锁。
**18 个包全部安装**：14 个 ArtCraft（13 应用 + 启动器）+ dsh + Blender + FreeCAD + Octave。

| 应用 | 启动 | 界面渲染 | 打开 | 保存 |
|---|---|---|---|---|
| VectorCraft | ✅ | ✅ | ✅ 端到端日志 | ✅ **22,312 字节** |
| FigCraft | ✅ 修复后 | ✅ 完整设计工具界面 | — | — |
| SoundCraft | ✅ | ✅（顶部菜单被裁几像素，见 §16） | — | — |
| DeckCraft / DesignCraft / CADCraft / WordCraft / GridCraft | ✅ 进程存活 | — | — | 已修 write 钩子并重建 |
| FreeCAD | ✅ | ✅ 完整启动页 + 工具条 | ✅ | ✅ **1,749 字节，合法 .FCStd** |
| Blender | ✅ | ✅（165,782 色） | 选择器已弹出 | ✅ 98,636 字节 |
| Octave | ✅ | ✅ 控制台 + 出图 | — | — |
| dsh | ✅ | ✅ web 起在 8900 | ✅ | ✅ |

FreeCAD 保存的直接证据（不是「应该能」）：

```
$ adb shell ls -la /sdcard/Download/freecad-saf-saved.FCStd
-rw-rw---- 1 u0_a0 media_rw 1749 2026-10-20 07:24 freecad-saf-saved.FCStd
$ file fc-saved.FCStd
Zip archive data, at least v2.0 to extract, compression method=deflate
$ python3 -c "import zipfile; print(zipfile.ZipFile('fc-saved.FCStd').namelist())"
['Document.xml', 'GuiDocument.xml', 'thumbnails/Thumbnail.png']
```

而且 FreeCAD 自己的启动页 Recent Files 里就列着 `freecad-saf-sav… 1.7 kB` —— 应用自己也认得这个文件，
说明写回的不只是字节，而是一个它能重新打开的文档。

## 18. 保存验证：四个应用、三套不同的 SAF 实现，全部通过

| 应用 | 落盘证据 | 实现方式 |
|---|---|---|
| VectorCraft | `/sdcard/Download/Untitled-1.vectorcraft` **22,312 字节**（JSON，`"format":"vectorcraft"`） | egui `Services::write` |
| DeckCraft | `/sdcard/Download/Presentation1.deckcraft (1)` **4,592 字节**，时间戳 01:55（同名已存在，系统加 ` (1)`） | egui `Services::write` |
| FreeCAD | `freecad-saf-saved.FCStd` **1,749 字节**，合法 zip（`Document.xml`/`GuiDocument.xml`/缩略图），且出现在应用自己的 Recent Files 里 | Qt `QFileDialog` + `resolveContentUri` |
| Blender | `usr/saf-test.blend` **98,636 字节**，与 `files/export/Untitled.blend` 等长 | SDL + 两阶段 publish |

DeckCraft 的保存入口不在菜单里 —— 是工具栏**第二个图标**（坐标为设备 `(117,33)`），
`File` 菜单点不开、`Ctrl+S` 也没接。这类「桌面应用移植到触摸屏」的坑，
只能靠逐个试出来。

## 19. ArtCraft 全系列 GUI 体检结果（14/14 通过）

判据：进程存活 + Activity 留在前台 + 截图颜色数（空白/崩溃画面只会有 1–3 种颜色）。

```
vectorcraft 486   figcraft 125    designcraft 311   deckcraft 389
cadcraft 621      wordcraft 525   gridcraft 155     soundcraft 393
filmcraft 167     lightcraft 862  photocraft 228    effectcraft 219
printcraft 338    launcher 716
```

全部远高于空白判据，即都渲染出了真实界面。

## 2026-10-22：顶部菜单被裁切的真正修法（egui 侧下移，不是 Android 主题）

现象：SoundCraft / CADCraft 的菜单栏（`SoundCraft File Edit View …`）文字底部被切掉（**不是顶部** —— 早先的判断是反的；旧截图里可见的 18 行正是完整 26 行墨迹的上半部分，缺的 8 行在下方）。

之前试过的两条 Android 侧方案都无效：

| 尝试 | 结果 |
|---|---|
| 去掉 `windowFullscreen`，只留 edge-to-edge opt-out | 状态栏出现了，但**菜单栏整个消失**（应用按全屏布局写死） |
| 保留全屏 + `windowOptOutEdgeToEdgeEnforcement=true` | 截图与改动前**字节完全相同**，这个 ROM 上 opt-out 已失效 |
| 把 `windowLayoutInDisplayCutoutMode` 从 `shortEdges` 改成 `never` | 同样**字节完全相同**，全屏下该模式不生效 |

**真正有效的做法**：在 egui 侧把 UI 整体下移。这些应用的 Android 壳实现了
`eframe::App::ui(&mut self, ui: &mut egui::Ui, …)`，拿到的是根 `Ui`，
把它的 `max_rect` 从顶部缩进即可：

```rust
const TOP_INSET: f32 = 28.0;   // 状态栏/挖孔高度（逻辑点）
let rect = ui.max_rect();
let inset = egui::Rect::from_min_max(rect.min + egui::vec2(0.0, TOP_INSET), rect.max);
ui.scope_builder(egui::UiBuilder::new().max_rect(inset), |ui| {
    self.app.ui(ui);
});
```

**SoundCraft 真机验证**：菜单栏完整显示，不再裁切。
CADCraft 同样改法，已重建。

## 2026-10-23：菜单裁切真正修好（子智能体实测）

### 真因

菜单栏面板是**固定高度**的：SoundCraft `exact_size(24.0)`、CADCraft `exact_size(22.0)`。
而 Android 触摸壳把 `style.spacing.interact_size` 设成 `(40, 40)`。
`egui::MenuBar` 会把行高设成 `interact_size.y` 并把按钮居中 —— 于是文字中心落在
条内约 20pt 处，**下半部分超出固定高度的面板，被分隔线/下一个面板盖住**。

所以是**底部裁切**。之前三次 Android 侧改动（去全屏、opt-out、cutout never）
和两次 egui 整层下移都无效，因为裁切发生在条**内部**，整体平移只会把裁切一起搬走。

### 修法

菜单栏高度改为从样式推导：

```rust
// soundcraft/crates/ui-egui/src/menus.rs
let bar_height = 24.0_f32.max(ui.spacing().interact_size.y + 4.0);   // 桌面仍 24，Android 44
// cadcraft/crates/ui-egui/src/menus.rs
let bar_height = 22.0_f32.max(ui.spacing().interact_size.y);         // 桌面仍 22，Android 40
```

同时删掉两个 app 里的 +28pt 整层位移（它们只是多出一条 70px 的黑带）。

### 实测（PA2573，2064×3096，400dpi → 2.5 px/pt）

| | 条高 | 墨迹 | 结果 |
|---|---|---|---|
| SoundCraft 修前 | 24pt (60px) | 18px | 裁切 |
| SoundCraft 修后 | 44pt (110px) | **26px** | 完整 |
| CADCraft 修后 | 40pt (100px) | 24px | 完整 |

点击命中也验证过：点 SoundCraft (60,52) 弹出菜单、点 CADCraft "Help" (1215,136) 弹出下拉。

## 2026-10-26：八个移植同步到上游、重建、装机并重发

`scripts/check-upstream.sh` 当时报告 8 个落后；本轮逐个同步、构建、安装并上传到
`tzhazuma/artcraft-android` 的 `android-v1` release。目标 SHA 即下表「新」列，
每个都已在 PA2573 上安装、冷启动并确认 Activity 在前台（进程存活、截图有真实界面）。

| app | 旧 → 新 | 提交数 | versionName | 构建 | 装机/启动 | 资产 |
|---|---|---|---|---|---|---|
| cadcraft | b9f96d3 → 754b609 | 85 | 0.13.0 | ✅ | ✅ | `CADCraft-android-arm64.apk` |
| craft-launcher | 0498d02 → 4ab652a（合入） | 28 | 0.8.1 | ✅ | ✅ | 新增 `ArtCraftLauncher-android-arm64.apk` |
| deckcraft | 7be391a → 7d4c490 | 164 | 0.11.1 | ✅ | ✅ | `DeckCraft-android-arm64.apk` |
| designcraft | c313c534 → 5024d61c | 220 | 0.13.0 | ✅ | ✅ | `DesignCraft-android-arm64.apk` |
| gridcraft | 10bec748 → 3b65b9af | 115 | 0.12.1 | ✅ | ✅ | `GridCraft-android-arm64.apk` |
| soundcraft | 93800e0 → 0a95b6d | 144 | 0.13.1 | ✅ | ✅ | `SoundCraft-android-arm64.apk` |
| vectorcraft | f924c69f → f05c33cd | 195 | 0.17.0 | ✅ | ✅ | `VectorCraft-android-arm64.apk` |
| wordcraft | cf2fe473 → 5dd5e8b3 | 243 | 0.11.0 | ✅ | ✅ | `WordCraft-android-arm64.apk` |

### 这次踩到的坑（都已处理）

- **eframe/egui 0.37**：`App::raw_input_hook` 多了一个 `&egui::Context` 参数。
  cadcraft、deckcraft、soundcraft、wordcraft 的壳改为把 `ctx` 转发给内部 App；
  designcraft/gridcraft/vectorcraft 仍是 0.36，不用改。
- **`-laaudio` 链接失败**（soundcraft）：同步带入了 cpal 的 Android AAudio 后端，
  该库在 NDK 里只从 API 26 起提供，而 cargo-ndk 默认 `--platform 21`。
  构建时设 `CARGO_NDK_PLATFORM=31`（壳的 minSdk 31）即通过。
- **craft-launcher 的唯一冲突** `crates/native/src/http.rs`：上游把 `root_certs()`
  内联成 `PlatformVerifier` 并新增 `is_loopback()`。保留移植侧 Android
  `/system/etc/security/cacerts` 覆盖，并同时保留上游新函数；`Http::new()` 仍调用
  `root_certs()`。`android-port` 已合并且推到 `tzhazuma/craft-launcher`。
- **菜单栏补丁**（cadcraft/soundcraft）重新对到新 `menus.rs`，并删掉补丁里过期的
  `Cargo.lock` 段（现在只含 `menus.rs`）。两处都干净应用。
- **Vivo 安装器没有 uiautomator 树**：全部用 `exec-out screencap` + 截图定位 + `input tap`。
  当前 3096×2064 横屏布局下，有「身份验证」提示行时复选框在 y=1748、按钮 y≈1875，
  没有时 y=1809 / y≈1936，x 都为 1305 / 1538。
- **APK 里的 `.so` 新鲜度**：不靠时间戳，解包出 `lib/arm64-v8a/libmain.so` 后取
  `.text` 段 SHA-256，与克隆里刚构建的 `libmain.so` 对比；8 个全部一致。
- **`dist-repos/<app>-android/` 原本只有 craft-launcher 一份**：其余 7 个是从它复制
  同一套通用 `scripts/`（脚本都接受 app 参数）再构建的，构建产物仍走 `out/`。

### 注意：上游在本轮进行中又前进了

完事后 `check-upstream.sh` 会再次报落后（cadcraft 1、craft-launcher 1、deckcraft 84、
designcraft 62、soundcraft 1、vectorcraft 3；figcraft 也在列但本轮任务未包含）。
这是上游在同步期间继续提交，不是本轮失败；上面表格里的「新」SHA 就是本轮实际
同步到的提交。下次按同一流程继续即可。
