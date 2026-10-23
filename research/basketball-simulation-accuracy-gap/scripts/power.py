import numpy as np, math
Phi=lambda x: 0.5*(1+math.erf(x/math.sqrt(2)))
rng=np.random.default_rng(1)
# Multi-class Brier (sum over 16 teams of (p-o)^2), uniform baseline:
print("uniform Brier", 15*(1/16)**2+(15/16)**2)
# The backtest's +/- values are standard errors over 21 seasons; the per-season SD behind them:
for name,se in [("Brier",0.089),("logloss",0.26)]:
    print(name,"per-season SD implied by the backtest SE:",round(se*math.sqrt(21),3))
# Favourite win rate 6/21 Wilson 95% CI
n,k=21,6; p=k/n; z=1.96
c=(p+z*z/(2*n))/(1+z*z/n); h=z*math.sqrt(p*(1-p)/n+z*z/(4*n*n))/(1+z*z/n)
print("fav 6/21 Wilson CI",round(c-h,3),round(c+h,3))
# Simulation: calibrated forecaster with champion probs drawn like a typical bracket
# generate 16-team probability vectors with a geometric tail; the favourite is 0.30 before renormalisation, 0.35 after
def probs(fav):
    r=np.array([fav*(0.65**i) for i in range(16)]); return r/r.sum()
P=probs(0.30)
print("fav prob",P[0].round(3),"entropy-ish mean champion prob", (P**2).sum().round(3))
S=200000
champ=rng.choice(16,size=S,p=P)
O=np.zeros((S,16)); O[np.arange(S),champ]=1
brier=((P-O)**2).sum(1); ll=-np.log(P[champ])
print("calibrated forecaster: E[Brier]",brier.mean().round(3),"SD per season",brier.std().round(3),
      "E[logloss]",ll.mean().round(3),"SD",ll.std().round(3),"P(fav wins)",P[0].round(3))
# Paired comparison: forecaster B is a sharpened/flattened version of truth P
def temper(P,t):
    q=P**t; return q/q.sum()
for t in [0.75,0.9,1.1,1.25,1.5]:
    Q=temper(P,t)
    d=((Q-O)**2).sum(1)-brier       # per-season paired Brier difference
    dl=-np.log(Q[champ])-ll
    for n in [21,50,100]:
        se=d.std()/math.sqrt(n); sel=dl.std()/math.sqrt(n)
        powb=1-Phi(1.96-abs(d.mean())/se)
        powl=1-Phi(1.96-abs(dl.mean())/sel)
        print(f"temper {t} n={n}: mean dBrier {d.mean():+.4f} SD {d.std():.3f} power {powb:.2f} | mean dLL {dl.mean():+.4f} SD {dl.std():.3f} power {powl:.2f}")
