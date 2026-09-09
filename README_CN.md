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

接收时直接写入目标文件，不创建 `.meta` 或 `.part`。重名时先选择保留两份、覆盖或跳过；选择覆盖后会清空原文件并重新接收。中途退出会留下已接收的部分内容。重新运行时，如果已有文件非空且小于源文件，会询问是否续传；双方校验已有部分的 SHA-256 一致后，只传剩余数据。校验失败则保留已有文件并停止，可重新接收并选择从头开始。等长或更大的文件仍按重名处理，不会仅凭大小自动跳过。只有文件长度符合预期并完成刷盘才报告成功。

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
