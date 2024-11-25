# Flash-Cat

Securely send the file folder from one computer to another computer.

- **symmetric encryption** (using aes-gcm)
- allows **multiple file** transfers
- easy **cross-platform** transfers (Windows, Linux, Mac)

![dream_TradingCard](./flash-cat-demo.gif)

## Install

### Download for your system
```bash
https://github.com/yunis-du/flash-cat/releases
```

### Use yum install
Systems for yum package managers, such as CentOS, RockyLinux, etc. Currently, only amd64 and arm64 architectures are supported.

```bash
sudo curl -o /etc/yum.repos.d/flash-cat.repo http://repo.duyunzhi.cn/rpm/flash-cat/flash-cat.repo && sudo yum install flash-cat -y
```

### Use apt-get install

```bash
echo "deb [trusted=yes] http://repo.duyunzhi.cn/apt/ flash-cat main" | sudo tee /etc/apt/sources.list.d/flash-cat.list && sudo apt-get update && sudo apt-get install flash-cat
```

### On macOS you can install the latest release with Homebrew
```bash
brew tap yunis-du/brew
brew install flash-cat
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