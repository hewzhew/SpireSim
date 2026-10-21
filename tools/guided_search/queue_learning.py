"""Offline scheduling preference; unchosen edges have unknown outcome quality."""
import numpy as np
from fractions import Fraction
from scipy.special import logsumexp
from path_learning import DecisionFeatures, canonical


class QueueDataset(DecisionFeatures):
    metric = "mean_queue_loss"
    objective = "mean_root mean_query mean_snapshot log(sum_F exp(-C)/sum_G exp(-C))"

    def __init__(self, records):
        self.records=[]; seen={}; families={}
        for r in records:
            if r.get("schema")!=1 or r.get("evidence_kind")!="witnessed_queue":
                raise ValueError("expected witnessed-queue evidence")
            root,family=r["root_id"],r["family"]
            if not root or not family or families.setdefault(root,family)!=family:
                raise ValueError("missing or conflicting family")
            key=(root,canonical(r["goal"]),r["snapshot_id"])
            value=canonical(r)
            if key in seen:
                if seen[key]!=value: raise ValueError("inconsistent duplicate queue")
                continue
            seen[key]=value; self.records.append(r)
        if not self.records: raise ValueError("no witnessed queue samples")
        self.queries={}; features=[]; parents=[]; incoming=[]; edge_parents=[]; choices=[]
        self.samples=[]; depths=[]; sequences=[]; witnessed=[]
        for i,r in enumerate(self.records):
            self.queries.setdefault((r["root_id"],canonical(r["goal"])),[]).append(i)
            base=len(features); node_depth=[]
            nodes,edges=r["nodes"],r["edges"]
            if not nodes or not edges: raise ValueError("empty queue tree")
            for n,node in enumerate(nodes):
                p,c=node["parent"],node["incoming_choice"]
                if n==0:
                    if p is not None or c is not None: raise ValueError("queue tree needs an explicit root")
                    parents.append(-1); incoming.append(-1); node_depth.append(0)
                else:
                    if type(p) is not int or not 0<=p<n or type(c) is not int or not 0<=c<len(nodes[p]["features"]["rows"]):
                        raise ValueError("invalid source-bound tree parent/choice")
                    parents.append(base+p); incoming.append(c); node_depth.append(node_depth[p]+1)
                features.append(node["features"])
            first=len(edge_parents); keys=set(); seq=set(); used=set()
            for e in edges:
                p,c=e["parent"],e["choice"]
                if type(p) is not int or not 0<=p<len(nodes) or type(c) is not int or not 0<=c<len(nodes[p]["features"]["rows"]):
                    raise ValueError("invalid queue edge")
                if type(e["sequence"]) is not int or not 0<=e["sequence"]<2**64 or (p,c) in keys or e["sequence"] in seq:
                    raise ValueError("duplicate edge or invalid source sequence")
                if e["depth"]!=node_depth[p]+1 or not isinstance(e["witnesses"],list) or any(type(w) is not int or w<0 for w in e["witnesses"]):
                    raise ValueError("invalid depth or witness references")
                keys.add((p,c)); seq.add(e["sequence"]); used.add(p)
                edge_parents.append(base+p); choices.append(c)
                depths.append(e["depth"]); sequences.append(e["sequence"]); witnessed.append(bool(e["witnesses"]))
            for n in sorted(used,reverse=True):
                while n>0:
                    n=nodes[n]["parent"]; used.add(n)
            if len(used)!=len(nodes): raise ValueError("unused tree nodes in queue sample")
            last=len(edge_parents)
            if not any(witnessed[first:last]): raise ValueError("queue has no witnessed continuation")
            self.samples.append((first,last))
        super().__init__(features)
        self.parents=np.asarray(parents,dtype=np.int64)
        self.incoming=np.asarray([self.action_offsets[p]+c if p>=0 else -1 for p,c in zip(parents,incoming)],dtype=np.int64)
        self.edge_parents=np.asarray(edge_parents,dtype=np.int64)
        self.edge_actions=self.action_offsets[self.edge_parents]+choices
        self.depths=np.asarray(depths,dtype=np.int64)
        self.sequences=np.asarray(sequences,dtype=np.uint64)
        self.witnessed=np.asarray(witnessed,dtype=bool)
        root_queries={}
        for root,_ in self.queries: root_queries[root]=root_queries.get(root,0)+1
        self.sample_weights=np.zeros(len(self.records))
        self.rank_weights=[None]*len(self.records)
        for (root,_),indices in self.queries.items():
            denominator=len(root_queries)*root_queries[root]*len(indices)
            self.sample_weights[indices]=1/denominator
            for i in indices: self.rank_weights[i]=Fraction(1,denominator)

    def costs(self, logs):
        maxima=np.full(len(self.parents),-np.inf)
        np.maximum.at(maxima,self.action_decision,logs)
        winners=np.full(len(maxima),len(logs),dtype=np.int64)
        matches=np.flatnonzero(logs==maxima[self.action_decision])
        np.minimum.at(winners,self.action_decision[matches],matches)
        prefix=np.zeros(len(self.parents))
        for n,p in enumerate(self.parents):
            if p>=0: prefix[n]=prefix[p]+(maxima[p]-logs[self.incoming[n]])
        return prefix[self.edge_parents]+(maxima[self.edge_parents]-logs[self.edge_actions]),winners

    def ranks(self,costs):
        result=[]
        for first,last in self.samples:
            order=np.lexsort((self.sequences[first:last],-self.depths[first:last],costs[first:last]))
            result.append(int(np.flatnonzero(self.witnessed[first:last][order])[0])+1)
        return np.asarray(result,dtype=np.int64)

    def checkpoint_key(self,loss,ranks):
        # Mathematically equal mean ranks must remain ties regardless of which
        # sample improved. Floating dot products can turn 1.2 into 1.2000000000000002.
        return sum(w*int(r) for w,r in zip(self.rank_weights,ranks)),loss

    def evaluate(self,weights):
        group_logs,member_logs,logs=self.forward(weights)
        costs,winners=self.costs(logs)
        edge_coeff=np.zeros(len(costs)); loss=0.
        for (first,last),weight in zip(self.samples,self.sample_weights):
            pos=np.flatnonzero(self.witnessed[first:last])+first
            all_z=logsumexp(-costs[first:last]); pos_z=logsumexp(-costs[pos])
            loss+=weight*(all_z-pos_z)
            edge_coeff[first:last]-=weight*np.exp(-costs[first:last]-all_z)
            edge_coeff[pos]+=weight*np.exp(-costs[pos]-pos_z)
        node_coeff=np.zeros(len(self.parents)); action_coeff=np.zeros(len(logs))
        np.add.at(node_coeff,self.edge_parents,edge_coeff)
        np.add.at(action_coeff,winners[self.edge_parents],edge_coeff)
        np.add.at(action_coeff,self.edge_actions,-edge_coeff)
        for n in range(len(self.parents)-1,-1,-1):
            p=self.parents[n]
            if p>=0:
                node_coeff[p]+=node_coeff[n]
                action_coeff[winners[p]]+=node_coeff[n]
                action_coeff[self.incoming[n]]-=node_coeff[n]
        gradient=self.backward(action_coeff,group_logs,member_logs)
        if not np.isfinite(loss) or not np.isfinite(gradient).all(): raise ValueError("nonfinite queue objective")
        return float(loss),gradient,costs,self.ranks(costs)
