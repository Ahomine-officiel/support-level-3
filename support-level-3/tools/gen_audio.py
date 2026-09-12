#!/usr/bin/env python3
"""Génère tous les sons du jeu en WAV 22050 Hz 16 bits mono (synthèse numpy)."""
import numpy as np, wave, os

SR = 22050
OUT = "/home/z/my-project/support-level-3/assets/audio"
os.makedirs(OUT, exist_ok=True)
rng = np.random.default_rng(7)

def save(name, x, gain=0.9):
    x = np.asarray(x, np.float32)
    m = np.max(np.abs(x)) or 1.0
    x = (x / m * gain * 32767).astype(np.int16)
    with wave.open(f"{OUT}/{name}.wav", "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(x.tobytes())
    print("son:", name)

def t(dur): return np.linspace(0, dur, int(SR * dur), endpoint=False)

def env(n, a=0.01, r=0.3, dur=None):
    """Enveloppe attack/release linéaire."""
    if dur is None: dur = n / SR
    e = np.ones(n)
    na, nr = int(a * SR), int(r * SR)
    na, nr = min(na, n), min(nr, n)
    if na > 0: e[:na] = np.linspace(0, 1, na)
    if nr > 0: e[-nr:] *= np.linspace(1, 0, nr)
    return e

def lowpass(x, alpha):
    y = np.empty_like(x)
    acc = 0.0
    for i, v in enumerate(x):
        acc += alpha * (v - acc)
        y[i] = acc
    return y

# ---------- Ambiance : bourdonnement sous-sol (boucle 20 s) ----------
dur = 20.0
x = t(dur)
brown = np.cumsum(rng.normal(0, 1, len(x))); brown -= np.linspace(brown[0], brown[-1], len(x))
brown = lowpass(brown, 0.02) * 0.4
hum = 0.5 * np.sin(2 * np.pi * 50 * x) + 0.25 * np.sin(2 * np.pi * 100 * x) + 0.12 * np.sin(2 * np.pi * 150 * x)
lfo = 0.75 + 0.25 * np.sin(2 * np.pi * x / 20.0 * 2)
vent = lowpass(rng.normal(0, 1, len(x)), 0.045) * 1.6
clank_t = [4.2, 11.7, 16.9]
clanks = np.zeros(len(x))
for ct_ in clank_t:
    i0 = int(ct_ * SR)
    n = int(0.4 * SR)
    if i0 + n < len(x):
        tt = t(0.4)
        clanks[i0:i0+n] += 0.5 * np.sin(2 * np.pi * (170 + rng.integers(-40, 40)) * tt) * np.exp(-tt * 12)
save("ambience", (brown + hum * lfo + vent) * env(len(x), 0.5, 0.5) * 0.5, 0.55)

# ---------- Bourdonnement néon (boucle 8 s) ----------
x = t(8.0)
buzz = 0.6 * np.sign(np.sin(2 * np.pi * 100 * x)) * 0.3 + 0.4 * np.sin(2 * np.pi * 100 * x)
buzz += lowpass(rng.normal(0, 1, len(x)), 0.3) * 0.15
am = 0.8 + 0.2 * np.sin(2 * np.pi * 7.3 * x)
save("fluorescent", buzz * am * 0.35, 0.4)

# ---------- Battement de cœur (1 shot, rejoué par le client) ----------
def thump(f0, dur=0.28):
    x = t(dur)
    return np.sin(2 * np.pi * (f0 - 18 * x / dur) * x) * np.exp(-x * 11)
hb = np.zeros(int(1.1 * SR))
hb[: len(thump(52))] += thump(52)
i1 = int(0.32 * SR)
hb[i1:i1 + len(thump(44))] += 0.75 * thump(44)
save("heartbeat", hb, 0.85)

# ---------- Jumpscare ----------
x = t(1.6)
freqs = [180, 241, 317, 423]
s = sum(np.sin(2 * np.pi * f * x * (1 - 0.35 * x)) for f in freqs)
s += np.sign(np.sin(2 * np.pi * 122 * x)) * 0.6
noise = rng.normal(0, 1, len(x)) * np.exp(-x * 2.2) * 1.2
boom = np.sin(2 * np.pi * 46 * x) * np.exp(-x * 2.8) * 1.4
save("jumpscare", (s * env(len(x), 0.004, 0.9) + noise + boom), 0.95)

# ---------- Cri de l'Auditeur (début de poursuite) ----------
x = t(1.9)
f = 660 + 240 * np.sin(2 * np.pi * 3.1 * x) - 180 * x
ph = 2 * np.pi * np.cumsum(f) / SR
s = np.sign(np.sin(ph)) * 0.4 + np.sin(ph * 1.5) * 0.35 + lowpass(rng.normal(0, 1, len(x)), 0.4) * 0.5
save("screech", s * env(len(x), 0.03, 0.5), 0.8)

# ---------- Pas ----------
for i in range(3):
    x = t(0.22)
    n = lowpass(rng.normal(0, 1, len(x)), 0.12) * np.exp(-x * 26)
    th = np.sin(2 * np.pi * (95 + i * 8) * x) * np.exp(-x * 30)
    save(f"footstep{i+1}", n * 1.1 + th * 0.8, 0.7)
for i in range(2):  # pas de l'Auditeur : plus lourds
    x = t(0.3)
    n = lowpass(rng.normal(0, 1, len(x)), 0.06) * np.exp(-x * 18)
    th = np.sin(2 * np.pi * (58 + i * 6) * x) * np.exp(-x * 20) * 1.3
    drag = lowpass(rng.normal(0, 1, len(x)), 0.5) * np.exp(-x * 8) * 0.2
    save(f"entity_step{i+1}", n + th + drag, 0.85)

# ---------- Portes ----------
x = t(1.1)
groan = np.sin(2 * np.pi * (60 + 25 * x) * x) * env(len(x), 0.15, 0.5)
creak = lowpass(rng.normal(0, 1, len(x)), 0.25) * np.sin(2 * np.pi * 2.2 * x) * env(len(x), 0.1, 0.4)
save("door_open", groan * 0.7 + creak * 0.9, 0.6)
x = t(0.55)
slam = np.sin(2 * np.pi * 52 * x) * np.exp(-x * 16) * 1.5 + lowpass(rng.normal(0, 1, len(x)), 0.09) * np.exp(-x * 22) * 1.2
save("door_slam", slam, 0.9)

# ---------- Interactions ----------
x = t(0.7)
rustle = lowpass(rng.normal(0, 1, len(x)), 0.55) * np.sin(2 * np.pi * 9 * x) * env(len(x), 0.02, 0.3)
blip = np.sin(2 * np.pi * 880 * x[:int(0.09*SR)]) * env(int(0.09*SR), 0.005, 0.05)
p = rustle * 0.8
i0 = int(0.4 * SR)
p[i0:i0 + len(blip)] += blip * 0.5
save("pickup", p, 0.7)

x = t(0.45)
stamp = np.sin(2 * np.pi * 70 * x) * np.exp(-x * 24) * 1.4 + lowpass(rng.normal(0, 1, len(x)), 0.2) * np.exp(-x * 30)
save("stamp", stamp, 0.85)

x = t(0.5)
clack = np.sin(2 * np.pi * 240 * x) * np.exp(-x * 34) * 0.7
blip2 = np.sin(2 * np.pi * 1320 * x) * env(len(x), 0.005, 0.35) * 0.35
save("battery", clack + blip2, 0.7)

# reboot réussi : carillon 2 notes
x = t(0.9)
n1 = np.sin(2 * np.pi * 523 * x[:int(0.3*SR)]) * env(int(0.3*SR), 0.01, 0.2)
n2 = np.sin(2 * np.pi * 784 * x[int(0.25*SR):]) * env(len(x) - int(0.25*SR), 0.01, 0.5)
ch = np.zeros(len(x))
ch[: len(n1)] += n1
ch[int(0.25*SR):int(0.25*SR) + len(n2)] += n2 * 0.8
save("reboot_done", ch, 0.7)

# ---------- Blackout / courant ----------
x = t(1.4)
f = 120 * np.exp(-x * 2.2)
ph = 2 * np.pi * np.cumsum(f) / SR
sweep = np.sin(ph) * env(len(x), 0.01, 0.6)
save("blackout", sweep, 0.75)
x = t(0.8)
click = np.sign(np.sin(2 * np.pi * 60 * x)) * env(len(x), 0.002, 0.5) * 0.5
hum2 = np.sin(2 * np.pi * 100 * x) * env(len(x), 0.25, 0.3) * 0.5
save("lights_on", click + hum2, 0.6)

# disjoncteur : clac puissant
x = t(0.6)
b_ = np.sin(2 * np.pi * 80 * x) * np.exp(-x * 18) * 1.2 + lowpass(rng.normal(0, 1, len(x)), 0.3) * np.exp(-x * 14) * 0.5
save("breaker", b_, 0.85)

# ---------- Capture (game over joueur) ----------
x = t(1.4)
scream = np.sign(np.sin(2 * np.pi * (300 + 500 * x) * x)) * 0.4
boom2 = np.sin(2 * np.pi * 40 * x) * np.exp(-x * 2.5) * 1.5
noise2 = rng.normal(0, 1, len(x)) * np.exp(-x * 2) * 0.8
save("caught", scream * env(len(x), 0.004, 0.6) + boom2 + noise2, 0.95)

# ---------- Murmures (peur élevée, boucle 5 s) ----------
x = t(5.0)
base = lowpass(rng.normal(0, 1, len(x)), 0.5)
form = 0.5 * np.sin(2 * np.pi * 3.7 * x) + 0.3 * np.sin(2 * np.pi * 6.1 * x + 1.3) + 0.2 * np.sin(2 * np.pi * 2.2 * x + 0.4)
w = base * (0.4 + form)
w *= 0.5 + 0.5 * np.sin(2 * np.pi * x / 5.0 * 3)
save("whisper", w * env(len(x), 0.8, 0.8) * 0.6, 0.45)

print("\nSons générés dans", OUT)
