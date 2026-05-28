"""Shared classes for the bagged margin-weighted SVC parasol scripts.

The class definitions here must stay importable at runtime for joblib to
unpickle the saved data/svc_k1_model.joblib and data/svc_ek1_model.joblib
deliverables. Both svc_k1.py and svc_ek1.py import from this module.
"""

import numpy as np
from sklearn.base import BaseEstimator, TransformerMixin


class SignedLog1p(BaseEstimator, TransformerMixin):
    """y = sign(x) * log1p(|x|). Squashes the 1..1e19 feature range while
    preserving sign and rank."""
    def fit(self, X, y=None):
        return self

    def transform(self, X):
        X = np.asarray(X, dtype=np.float64)
        return np.sign(X) * np.log1p(np.abs(X))


class BagSVCPredictor:
    """Bagged margin-weighted SVC ensemble.

    Holds a fitted preprocessing pipeline (SignedLog1p + StandardScaler)
    and a list of fitted SVCs, each trained on a stratified bootstrap with
    margin-weighted training. predict() averages predict_proba across the
    SVCs and returns argmax.
    """
    def __init__(self, pre_pipe, svms, params):
        self.pre_pipe = pre_pipe
        self.svms = svms
        self.params = params

    def predict(self, X):
        Xs = self.pre_pipe.transform(np.asarray(X, dtype=np.float64))
        probs = np.zeros((Xs.shape[0], 2), dtype=np.float64)
        for svc in self.svms:
            probs += svc.predict_proba(Xs)
        return np.argmax(probs, axis=1)

    def predict_proba(self, X):
        Xs = self.pre_pipe.transform(np.asarray(X, dtype=np.float64))
        probs = np.zeros((Xs.shape[0], 2), dtype=np.float64)
        for svc in self.svms:
            probs += svc.predict_proba(Xs)
        return probs / len(self.svms)
