import csv, sys, math, collections, statistics as st


def main():
    if len(sys.argv) != 3:
        sys.exit("usage: python3 eval538b.py <nba_playoffs.csv> <nba_games.csv>")
    pp, gp = sys.argv[1], sys.argv[2]
    with open(pp, newline="", encoding="utf-8") as f:
        rows=list(csv.DictReader(f))
    by=collections.defaultdict(lambda: collections.defaultdict(dict))
    for r in rows: by[r['season']][r['forecast_date']][r['team']]=r
    pre={}
    for season in sorted(by):
        for d in sorted(by[season]):
            teams=by[season][d]
            certain=[t for t,r in teams.items() if float(r['make_playoffs'])==1.0]
            others=[t for t,r in teams.items() if 0<float(r['make_playoffs'])<1]
            if len(certain)==16 and not others and all(0<float(teams[t]['make_conf_semis'])<1 for t in certain):
                pre[season]=d; break
    # 1. full 2018 and 2022 distributions
    for s in ['2018','2022']:
        teams=by[s][pre[s]]
        P=sorted(((float(r['win_finals']),t) for t,r in teams.items() if float(r['make_playoffs'])==1.0),reverse=True)
        print(s,pre[s],[(t,round(p,3)) for p,t in P])
    # 2. pooled rounds at pre-playoff date: 4 binary items per team (semis, conf finals, finals, title)
    items=['make_conf_semis','make_conf_finals','make_finals','win_finals']
    bins=collections.defaultdict(lambda:[0,0.0,0])
    per_season={}
    allb=[]
    for s in sorted(by):
        teams=by[s][pre[s]]; bs=[]
        for t,r in teams.items():
            if float(r['make_playoffs'])!=1.0: continue
            for it in items:
                p=float(r[it]); o=float(r[it+'_outcome']); bs.append((p-o)**2)
                b=min(int(p*10),9); bins[b][0]+=1; bins[b][1]+=p; bins[b][2]+=o
        per_season[s]=st.mean(bs); allb+=bs
    print("\nPooled-rounds binary Brier per season (64 items each):",{s:round(v,4) for s,v in per_season.items()})
    v=list(per_season.values())
    print(f"mean {st.mean(v):.4f} SD {st.pstdev(v):.4f} SE {st.stdev(v)/math.sqrt(len(v)):.4f}; N items {len(allb)}")
    print("Calibration bins (pred decile: n, mean pred, observed rate):")
    for b in sorted(bins):
        n,sp,so=bins[b]; print(f"  {b/10:.1f}-{(b+1)/10:.1f}: n={n} pred={sp/n:.3f} obs={so/n:.3f}")
    # 3. game-level: regular season vs playoffs favourite win rate and Brier
    with open(gp, newline="", encoding="utf-8") as f:
        g=list(csv.DictReader(f))
    stats=collections.defaultdict(lambda:[0,0,0.0,0.0])  # n, favwins, brier sum, mean p_fav sum
    for r in g:
        s=r['season']
        if s not in pre: continue
        phase='playoffs' if r['date']>pre[s] else 'regular'
        p1=float(r['prob1']); o1=float(r['prob1_outcome'])
        pf=max(p1,1-p1); fw=1 if (p1>=0.5)==(o1==1) else 0
        k=stats[phase]; k[0]+=1; k[1]+=fw; k[2]+=(p1-o1)**2; k[3]+=pf
        k=stats[(s,phase)]; k[0]+=1; k[1]+=fw; k[2]+=(p1-o1)**2; k[3]+=pf
    print("\nGame-level (seasons 2016-2022 as in 538 data):")
    for k in ['regular','playoffs']:
        n,fw,bsum,pfs=stats[k]; print(f"  {k}: n={n} fav won {fw/n:.3f} mean p_fav {pfs/n:.3f} Brier {bsum/n:.4f}")
    for s in sorted(pre):
        for k in ['regular','playoffs']:
            n,fw,bsum,pfs=stats[(s,k)]; print(f"  {s} {k}: n={n} fav won {fw/n:.3f} mean p_fav {pfs/n:.3f} Brier {bsum/n:.4f}")


if __name__ == "__main__":
    main()
