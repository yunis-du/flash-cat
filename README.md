# Flash-Cat

Securely send the file folder from one computer to another computer.

- **symmetric encryption** (using aes-gcm)
- allows **multiple file** transfers
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
Systems for yum package managers, such as CentOS, RockyLinux, etc. Currently, only amd64 and arm64 architectures are supported.

```bash
sudo curl -o /etc/yum.repos.d/flash-cat.repo http://repo.yunisdu.com/rpm/flash-cat/flash-cat.repo && sudo yum install flash-cat -y
```

### Use apt-get install
Systems for apt package manager-based (such as Debian, Ubuntu, and their derivatives).

```bash
echo "deb [trusted=yes] http://repo.yunisdu.com/apt/ flash-cat main" | sudo tee /etc/apt/sources.list.d/flash-cat.list && sudo apt-get update && sudo apt-get install flash-cat
```

### On macOS you can install the latest release with Homebrew
For macOS, use Homebrew package Manager to install the latest version of flash-cat.

```bash
brew tap yunis-du/brew
brew install flash-cat
```

### Or, you can install Cargo and build from source (requires Cargo 1.80+)

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

## Deployment your owner relay
...