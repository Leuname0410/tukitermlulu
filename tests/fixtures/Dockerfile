FROM alpine:3.22

RUN apk add --no-cache openssh bash zsh fish \
    && adduser -D -s /bin/sh tuki \
    && adduser -D -s /bin/bash tuki-bash \
    && adduser -D -s /bin/zsh tuki-zsh \
    && adduser -D -s /usr/bin/fish tuki-fish \
    && for user in tuki tuki-bash tuki-zsh tuki-fish; do \
        echo "$user:tuki-test" | chpasswd; \
        mkdir -p /home/$user/.ssh; \
        chown -R $user:$user /home/$user/.ssh; \
        chmod 700 /home/$user/.ssh; \
    done \
    && printf 'HISTCONTROL=ignorespace\n' > /home/tuki-bash/.profile \
    && chown tuki-bash:tuki-bash /home/tuki-bash/.profile \
    && mkdir -p /home/tuki-fish/.config/fish \
    && printf '%s\n' 'if not status is-interactive' '    echo "fish: \$0 is not valid in this context" >&2' 'end' > /home/tuki-fish/.config/fish/config.fish \
    && chown -R tuki-fish:tuki-fish /home/tuki-fish/.config

COPY ed25519.pub /home/tuki/.ssh/authorized_keys
# Solo se copian las claves públicas: las privadas nunca entran en la imagen.
COPY keys/*.pub /tmp/tuki-pubkeys/
RUN cat /home/tuki/.ssh/authorized_keys /tmp/tuki-pubkeys/*.pub > /home/tuki/.ssh/authorized_keys.all \
    && mv /home/tuki/.ssh/authorized_keys.all /home/tuki/.ssh/authorized_keys \
    && rm -rf /tmp/tuki-pubkeys \
    && chown tuki:tuki /home/tuki/.ssh/authorized_keys \
    && chmod 600 /home/tuki/.ssh/authorized_keys

COPY host/ssh_host_ed25519_key /etc/ssh/ssh_host_ed25519_key
RUN chmod 600 /etc/ssh/ssh_host_ed25519_key

# Configuración mínima: solo autenticación por clave y contraseña, sin root.
RUN printf '%s\n' \
    'HostKey /etc/ssh/ssh_host_ed25519_key' \
    'PasswordAuthentication yes' \
    'PubkeyAuthentication yes' \
    'PermitRootLogin no' \
    'Subsystem sftp internal-sftp' \
    > /etc/ssh/sshd_config

EXPOSE 22

CMD ["/usr/sbin/sshd", "-D", "-e"]
