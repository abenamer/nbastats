import csv, sys, math, collections


def main():
    if len(sys.argv) != 2:
        sys.exit("usage: python3 eval538.py <nba-forecast CSV from fivethirtyeight/checking-our-work-data>")
    path=sys.argv[1]
    with open(path, newline="", encoding="utf-8") as f:
        rows=list(csv.DictReader(f))
    by=collections.defaultdict(lambda: collections.defaultdict(dict))
    for r in rows:
        by[r['season']][r['forecast_date']][r['team']]=r
    print("seasons:",sorted(by))
    results=[]
    for season in sorted(by):
        dates=sorted(by[season])
        pre=None
        for d in dates:
            teams=by[season][d]
            certain=[t for t,r in teams.items() if float(r['make_playoffs'])==1.0]
            others=[t for t,r in teams.items() if 0<float(r['make_playoffs'])<1]
            if len(certain)==16 and not others:
                # ensure no first-round series decided yet: make_conf_semis strictly between 0 and 1 for all 16
                und=[t for t in certain if 0<float(teams[t]['make_conf_semis'])<1]
                if len(und)==16:
                    pre=d; break
        if pre is None:
            print(season,"no clean pre-playoff date; dates",dates[0],dates[-1]); continue
        teams=by[season][pre]
        P={t:float(r['win_finals']) for t,r in teams.items() if float(r['make_playoffs'])==1.0}
        champ=[t for t,r in teams.items() if r['win_finals_outcome']=='1.0']
        s=sum(P.values())
        brier=sum((p-(1 if t in champ else 0))**2 for t,p in P.items())
        pc=P[champ[0]] if champ else float('nan')
        fav=max(P,key=P.get)
        ll=-math.log(pc) if pc>0 else float('inf')
        results.append((season,pre,fav,round(P[fav],3),champ[0] if champ else None,round(pc,3),round(brier,3),round(ll,3),round(s,3)))
        print(f"{season} pre={pre} fav={fav} p_fav={P[fav]:.3f} champ={champ[0] if champ else None} p_champ={pc:.3f} Brier={brier:.3f} logloss={ll:.3f} sumP={s:.3f}")
    n=len(results)
    import statistics as st
    B=[r[6] for r in results]; L=[r[7] for r in results]; PC=[r[5] for r in results]
    favwins=sum(1 for r in results if r[2]==r[4])
    print(f"\nN={n} seasons; mean Brier={st.mean(B):.3f} (SD {st.pstdev(B):.3f}, SE {st.stdev(B)/math.sqrt(n):.3f}); mean logloss={st.mean(L):.3f} (SD {st.pstdev(L):.3f}, SE {st.stdev(L)/math.sqrt(n):.3f}); favourite won {favwins}/{n}; mean p(champion)={st.mean(PC):.3f}; mean p(favourite)={st.mean([r[3] for r in results]):.3f}")
    # uniform baseline Brier for 16 teams
    print("uniform Brier 0.9375, uniform logloss", round(math.log(16),3))


if __name__ == "__main__":
    main()
