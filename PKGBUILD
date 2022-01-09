# Maintainer:

pkgname=hpk
pkgver=0.3.9
pkgrel=1
pkgdesc="HPK archiver for Haemimont Engine game files (Tropico 3-5, Omerta, Victor Vran, Surviving Mars etc.) "
arch=('x86_64')
url="https://github.com/nickelc/hpk"
license=('GPL3')
depends=()
makedepends=('git' 'cargo')

source=("git+https://github.com/nickelc/hpk.git#tag=v$pkgver")
sha256sums=('SKIP')

build() {
    cd $srcdir/$pkgname
    
    cargo build --release
}

package() {
    cd $srcdir/$pkgname

    install -D -m755 target/release/hpk -t "$pkgdir"/usr/bin
}
