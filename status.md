# Terminal Status

Last update: Sat May  2 15:32:43 UTC 2026

## Last output
```
runner@runnervmeorf1:~/work/tel/tel$ ssh 3ALCYZAFR5CzZDgfPUKRxeGSw@sfo2.tmate.io
The authenticity of host 'sfo2.tmate.io (157.230.72.130)' can't be established.
RSA key fingerprint is SHA256:Hthk2T/M/Ivqfk1YYUn5ijC2Att3+UPzD7Rn72P5VWs.
This key is not known by any other names.
Are you sure you want to continue connecting (yes/no/[fingerprint])? yes

Warning: Permanently added 'sfo2.tmate.io' (RSA) to the list of known hosts.
Connection closed by 157.230.72.130 port 22
runner@runnervmeorf1:~/work/tel/tel$
runner@runnervmeorf1:~/work/tel/tel$ curl -fsSL https://pkg.cloudflareclient.com
/pubkey.gpg \ | sudo gpg --dearmor -o /usr/share/keyrings/cloudflare-warp.gpg
curl: (3) URL rejected: Malformed input to a URL function
runner@runnervmeorf1:~/work/tel/tel$
runner@runnervmeorf1:~/work/tel/tel$ echo "deb [signed-by=/usr/share/keyrings/cl
oudflare-warp.gpg] https://pkg.cloudflareclient.com/ $(lsb_release -cs) main" \
| sudo tee /etc/apt/sources.list.d/cloudflare-client.list
deb [signed-by=/usr/share/keyrings/cloudflare-warp.gpg] https://pkg.cloudflarecl
ient.com/ noble main
runner@runnervmeorf1:~/work/tel/tel$
runner@runnervmeorf1:~/work/tel/tel$
```
