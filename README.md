# Flash-Cat

English | [简体中文](./README_CN.md)

Securely send the file folder from one computer to another computer.

- **symmetric encryption** (using aes-gcm)
- support **multiple file** transfers
- support **resume transfer from breakpoint**
- easy **cross-platform** transfers (Windows, Linux, Mac)

![dream_TradingCard](./flash-cat-demo.gif)

## Install

### Automatic installation script

#### Linux/macOS
```bash
curl -fsSL https://raw.githubusercontent.com/yunis-du/flash-cat/master/install.sh | sudo bash
```
##### China mainland
```bash
curl -fsSL https://download.yunisdu.com/flash-cat/install_cn.sh | sudo bash
```

#### Windows(Run Powershell as Administrator)
```powershell
irm https://raw.githubusercontent.com/yunis-du/flash-cat/master/install.ps1 | iex
```
##### China mainland
```powershell
irm https://download.yunisdu.com/flash-cat/install_cn.ps1 | iex
```

### Use yum install
Systems for yum package managers, such as Fedora, RockyLinux, etc. Currently, only amd64 and arm64 architectures are supported.

```bash
sudo curl -o /etc/yum.repos.d/flash-cat.repo https://repo.yunisdu.com/rpm/flash-cat/flash-cat.repo && sudo yum install flash-cat -y
```

### Use apt-get install
Systems for apt package manager-based (such as Debian, Ubuntu, and their derivatives).

```bash
sudo curl -fsSL https://repo.yunisdu.com/apt/flash-cat-archive-keyring.gpg -o /usr/share/keyrings/flash-cat-archive-keyring.gpg &&
echo "deb [arch=amd64,arm64 signed-by=/usr/share/keyrings/flash-cat-archive-keyring.gpg] https://repo.yunisdu.com/apt/ flash-cat main" | sudo tee /etc/apt/sources.list.d/flash-cat.list && sudo apt-get update && sudo apt-get install flash-cat
```

### On macOS you can install the latest release with Homebrew
For macOS, use Homebrew package Manager to install the latest version of flash-cat.

```bash
brew tap yunis-du/brew
brew install flash-cat
```

### Or, you can install Cargo and build from source (requires Cargo 1.85+)

```bash
cargo install --git https://github.com/yunis-du/flash-cat flash_cat_cli
```

## Usage

### simple send and receive
send:
```bash
flash-cat send files or folder

...
Share code is: xx-xxxx-xxxx
...
```
receive:
```bash
flash-cat recv xx-xxxx-xxxx
```

## Deploy your own relay server

You can deploy your own relay server to handle file transfers within your local network or over the internet.

### Start relay server
```bash
flash-cat relay
```

## Specify relay

### command-line parameters
send:
```bash
flash-cat send files or folder --relay 127.0.0.1:6880

...
Share code is: xx-xxxx-xxxx
...
```
receive:
```bash
flash-cat recv xx-xxxx-xxxx --relay 127.0.0.1:6880
```

### environmental variable
send:
```bash
export FLASH_CAT_RELAY=127.0.0.1:6880
flash-cat send files or folder

...
Share code is: xx-xxxx-xxxx
...
```
receive:
```bash
export FLASH_CAT_RELAY=127.0.0.1:6880
flash-cat recv xx-xxxx-xxxx
```
