# Flash-Cat

安全地在两台计算机之间传输文件和文件夹。

- **对称加密** (使用 aes-gcm)
- 支持**多文件**传输
- 支持**断点续传**，包括退出后重新接收
- 简单的**跨平台**传输 (Windows, Linux, Mac)

![Flash Cat CLI 文件传输演示](./flash-cat-demo.gif)

## 安装

### 自动安装脚本

#### Linux/macOS
```bash
curl -fsSL https://raw.githubusercontent.com/yunis-du/flash-cat/master/install.sh | sudo bash
```
##### 中国大陆
```bash
curl -fsSL https://download.yunisdu.com/flash-cat/install_cn.sh | sudo bash
```

#### Windows(以管理员身份运行Powershell)
```powershell
irm https://raw.githubusercontent.com/yunis-du/flash-cat/master/install.ps1 | iex
```
##### 中国大陆
```powershell
irm https://download.yunisdu.com/flash-cat/install_cn.ps1 | iex
```

### 使用 yum 安装
yum包管理器系统，如Fedora、RockyLinux等。目前，只支持amd64和arm64架构

```bash
sudo curl -o /etc/yum.repos.d/flash-cat.repo https://repo.yunisdu.com/rpm/flash-cat/flash-cat.repo && sudo yum install flash-cat -y
```

### 使用 apt-get 安装
基于apt包管理器的系统（如Debian、Ubuntu及其衍生产品）。

```bash
sudo curl -fsSL https://repo.yunisdu.com/apt/flash-cat-archive-keyring.gpg -o /usr/share/keyrings/flash-cat-archive-keyring.gpg &&
echo "deb [arch=amd64,arm64 signed-by=/usr/share/keyrings/flash-cat-archive-keyring.gpg] https://repo.yunisdu.com/apt/ flash-cat main" | sudo tee /etc/apt/sources.list.d/flash-cat.list && sudo apt-get update && sudo apt-get install flash-cat
```

### 在macOS上，可以通过Homebrew安装最新版本
对于macOS，使用Homebrew软件包管理器安装最新版本的flash-cat。

```bash
brew tap yunis-du/brew
brew install flash-cat
```

### 或者，您可以安装Cargo并从源代码构建（需要Cargo 1.85+）

```bash
cargo install --git https://github.com/yunis-du/flash-cat flash_cat_cli
```

## 用法

### 简单的发送与接收
发送:
```bash
flash-cat send files or folder

...
Share code is: xx-xxxx-xxxx
...
```
接收:
```bash
flash-cat recv xx-xxxx-xxxx
```


### 仅在局域网内传输

使用 `--lan`（或 `-l`）发送时不会连接公共中继。两台设备需在同一局域网内，且局域网发现可用。此参数不能与 `--relay`（包括 `FLASH_CAT_RELAY` 环境变量）或 `--no-lan` 同时使用。

未指定 `--relay` 时，接收端默认优先自动发现局域网发送端，未发现时回退公共中继。使用 `recv --lan` 则仅通过局域网接收，发现失败直接报错，不连接公共中继，且不能与 `--relay` 同时使用。

```bash
flash-cat send --lan 文件或文件夹
flash-cat recv xx-xxxx-xxxx --lan
```

## 部署你自己的中继服务

您可以部署自己的中继服务器来处理本地网络或互联网上的文件传输。

### 启动中继服务
```bash
flash-cat relay
```

## 指定中继

### 通过命令行参数
发送:
```bash
flash-cat send files or folder --relay 127.0.0.1:6880

...
Share code is: xx-xxxx-xxxx
...
```
接收:
```bash
flash-cat recv xx-xxxx-xxxx --relay 127.0.0.1:6880
```

### 通过环境变量（仅发送端）
发送端可通过环境变量指定中继：
```bash
export FLASH_CAT_RELAY=127.0.0.1:6880
flash-cat send files or folder

...
Share code is: xx-xxxx-xxxx
...
```
接收端不会读取 `FLASH_CAT_RELAY`。通过私有中继接收时，请显式传入 `--relay`。
