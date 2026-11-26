#!/bin/sh
# Genera las llaves de prueba PEM/OpenSSH y PPK. No regenerar llaves ya creadas.
set -eu

cd "$(dirname "$0")"

if [ -d keys ]; then
    echo "tests/fixtures/keys ya existe: abortando para no regenerar las llaves." >&2
    exit 1
fi

# En Windows, la CLI de Docker necesita la ruta nativa para el bind mount.
case $(uname -s) in
    MINGW*|MSYS*|CYGWIN*)
        fixtures_dir=$(pwd -W 2>/dev/null || pwd)
        ;;
    *)
        fixtures_dir=$(pwd)
        ;;
esac

docker run --rm -v "$fixtures_dir":/k alpine:3.22 sh -c '
    set -eu
    apk add --no-cache -q putty openssh-keygen >/dev/null
    cd /k
    mkdir keys
    pass_file=/tmp/tuki-pass.txt
    printf "tuki-pass" > "$pass_file"

    ssh-keygen -q -t rsa -b 3072 -N "" -C tuki-test-rsa_openssh -f keys/rsa_openssh
    ssh-keygen -q -t rsa -b 3072 -N tuki-pass -C tuki-test-rsa_openssh_pass -f keys/rsa_openssh_pass
    ssh-keygen -q -t rsa -b 3072 -m PEM -N "" -C tuki-test-rsa_pkcs1.pem -f keys/rsa_pkcs1.pem
    ssh-keygen -q -t rsa -b 3072 -m PKCS8 -N "" -C tuki-test-rsa_pkcs8.pem -f keys/rsa_pkcs8.pem
    ssh-keygen -q -t ecdsa -b 256 -N "" -C tuki-test-ecdsa_p256 -f keys/ecdsa_p256
    ssh-keygen -q -t ecdsa -b 256 -N tuki-pass -C tuki-test-ecdsa_p256_pass -f keys/ecdsa_p256_pass
    ssh-keygen -q -t ed25519 -N tuki-pass -C tuki-test-ed25519_pass -f keys/ed25519_pass

    puttygen -q -t ed25519 -C tuki-test-ed25519_v2 -o keys/ed25519_v2.ppk --ppk-param version=2 --new-passphrase /dev/null
    puttygen -q -t ed25519 -C tuki-test-ed25519_v2_pass -o keys/ed25519_v2_pass.ppk --ppk-param version=2 --new-passphrase "$pass_file"
    puttygen -q -t ed25519 -C tuki-test-ed25519_v3 -o keys/ed25519_v3.ppk --ppk-param version=3 --new-passphrase /dev/null
    puttygen -q -t ed25519 -C tuki-test-ed25519_v3_pass -o keys/ed25519_v3_pass.ppk --ppk-param version=3 --new-passphrase "$pass_file"
    puttygen -q -t rsa -b 3072 -C tuki-test-rsa_v3 -o keys/rsa_v3.ppk --ppk-param version=3 --new-passphrase /dev/null

    for key in keys/*.ppk; do
        puttygen -q "$key" -O public-openssh -o "$key.pub"
    done

    chmod 600 keys/rsa_openssh keys/rsa_openssh_pass keys/rsa_pkcs1.pem keys/rsa_pkcs8.pem \
        keys/ecdsa_p256 keys/ecdsa_p256_pass keys/ed25519_pass keys/*.ppk
    chmod 644 keys/*.pub
    rm -f "$pass_file"
'
