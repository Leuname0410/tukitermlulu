FROM alpine:3.22

RUN apk add --no-cache openssh \
    && adduser -D -s /bin/sh tuki \
    && echo 'tuki:tuki-test' | chpasswd \
    && mkdir -p /home/tuki/.ssh \
    && chown -R tuki:tuki /home/tuki/.ssh \
    && chmod 700 /home/tuki/.ssh

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
