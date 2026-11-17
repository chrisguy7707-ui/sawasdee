package com.sawasdee.thai;

import android.app.Activity;
import android.content.Context;
import android.content.SharedPreferences;

import com.android.billingclient.api.AcknowledgePurchaseParams;
import com.android.billingclient.api.BillingClient;
import com.android.billingclient.api.BillingClientStateListener;
import com.android.billingclient.api.BillingFlowParams;
import com.android.billingclient.api.BillingResult;
import com.android.billingclient.api.PendingPurchasesParams;
import com.android.billingclient.api.ProductDetails;
import com.android.billingclient.api.Purchase;
import com.android.billingclient.api.QueryProductDetailsParams;
import com.android.billingclient.api.QueryPurchasesParams;

import java.util.Collections;
import java.util.List;

/**
 * One-time purchase "full_unlock" through Google Play Billing. No server and no sign-in: the purchase belongs to
 * the person's Google account, so it comes back by itself after a reinstall or on a new phone. At every start
 * (and on the "구매 복원" button) the app asks Play which purchases the account owns.
 *
 * The result of the last successful check is kept in SharedPreferences so the unlock also works offline.
 * A refund removes the purchase from Play's list, and the next check locks the units again.
 * The Rust side polls state(), price() and takeMessage().
 */
public class Billing {
    static final String PRODUCT = "full_unlock";
    private static final String PREFS = "billing";
    private static final String OWNED = "owned";

    private static BillingClient client;
    private static ProductDetails details;
    private static volatile boolean connected;
    private static volatile boolean pending;
    private static volatile String message = "";
    private static volatile boolean owned;
    private static boolean loaded;

    private static SharedPreferences prefs(Context c) {
        return c.getSharedPreferences(PREFS, Context.MODE_PRIVATE);
    }

    private static void setOwned(Context c, boolean v) {
        owned = v;
        prefs(c).edit().putBoolean(OWNED, v).apply();
    }

    /** Connects to Google Play (once) and checks what the account owns. */
    public static synchronized void init(final Activity a) {
        final Context app = a.getApplicationContext();
        if (!loaded) {
            owned = prefs(app).getBoolean(OWNED, false);
            loaded = true;
        }
        if (client != null) {
            refresh(a);
            return;
        }
        client = BillingClient.newBuilder(app)
                .setListener(new com.android.billingclient.api.PurchasesUpdatedListener() {
                    @Override
                    public void onPurchasesUpdated(BillingResult r, List<Purchase> list) {
                        int code = r.getResponseCode();
                        if (code == BillingClient.BillingResponseCode.OK && list != null) {
                            handle(app, list, true);
                        } else if (code == BillingClient.BillingResponseCode.ITEM_ALREADY_OWNED) {
                            queryOwned(app);
                        } else if (code == BillingClient.BillingResponseCode.USER_CANCELED) {
                            message = "";
                        } else {
                            message = "구매하지 못했어요. 잠시 뒤에 다시 시도해 주세요";
                        }
                    }
                })
                .enablePendingPurchases(PendingPurchasesParams.newBuilder().enableOneTimeProducts().build())
                .enableAutoServiceReconnection()
                .build();
        client.startConnection(new BillingClientStateListener() {
            @Override
            public void onBillingSetupFinished(BillingResult r) {
                connected = r.getResponseCode() == BillingClient.BillingResponseCode.OK;
                if (connected) {
                    queryOwned(app);
                    queryDetails();
                }
            }

            @Override
            public void onBillingServiceDisconnected() {
                connected = false;
            }
        });
    }

    /** "구매 복원": asks Play again which purchases the account owns. */
    public static void refresh(Activity a) {
        if (client == null) {
            init(a);
            return;
        }
        if (connected) {
            queryOwned(a.getApplicationContext());
            queryDetails();
        } else {
            client.startConnection(new BillingClientStateListener() {
                @Override
                public void onBillingSetupFinished(BillingResult r) {
                    connected = r.getResponseCode() == BillingClient.BillingResponseCode.OK;
                }

                @Override
                public void onBillingServiceDisconnected() {
                    connected = false;
                }
            });
        }
    }

    private static void queryOwned(final Context app) {
        client.queryPurchasesAsync(QueryPurchasesParams.newBuilder().setProductType(BillingClient.ProductType.INAPP).build(),
                new com.android.billingclient.api.PurchasesResponseListener() {
                    @Override
                    public void onQueryPurchasesResponse(BillingResult r, List<Purchase> list) {
                        if (r.getResponseCode() != BillingClient.BillingResponseCode.OK) {
                            return; // keep the saved answer (offline or Play busy)
                        }
                        boolean has = handle(app, list, false);
                        if (!has && !pending) {
                            setOwned(app, false); // refunded, or never bought
                        }
                    }
                });
    }

    /** Looks at purchases; unlocks on PURCHASED, confirms (acknowledges) them. Returns true if the product is owned. */
    private static boolean handle(final Context app, List<Purchase> list, boolean fresh) {
        boolean has = false;
        pending = false;
        for (final Purchase p : list) {
            if (!p.getProducts().contains(PRODUCT)) {
                continue;
            }
            if (p.getPurchaseState() == Purchase.PurchaseState.PURCHASED) {
                has = true;
                setOwned(app, true);
                if (!p.isAcknowledged()) {
                    // Without this Google refunds the purchase automatically after 3 days.
                    client.acknowledgePurchase(AcknowledgePurchaseParams.newBuilder().setPurchaseToken(p.getPurchaseToken()).build(),
                            new com.android.billingclient.api.AcknowledgePurchaseResponseListener() {
                                @Override
                                public void onAcknowledgePurchaseResponse(BillingResult r) {
                                }
                            });
                }
                if (fresh) {
                    message = "구매해 주셔서 감사해요! 모든 단원이 열렸어요";
                }
            } else if (p.getPurchaseState() == Purchase.PurchaseState.PENDING) {
                pending = true;
                if (fresh) {
                    message = "결제가 처리 중이에요. 끝나면 자동으로 열려요";
                }
            }
        }
        return has;
    }

    private static void queryDetails() {
        QueryProductDetailsParams.Product product = QueryProductDetailsParams.Product.newBuilder()
                .setProductId(PRODUCT).setProductType(BillingClient.ProductType.INAPP).build();
        client.queryProductDetailsAsync(QueryProductDetailsParams.newBuilder().setProductList(Collections.singletonList(product)).build(),
                new com.android.billingclient.api.ProductDetailsResponseListener() {
                    @Override
                    public void onProductDetailsResponse(BillingResult r, com.android.billingclient.api.QueryProductDetailsResult res) {
                        List<ProductDetails> l = res.getProductDetailsList();
                        if (r.getResponseCode() == BillingClient.BillingResponseCode.OK && l != null && !l.isEmpty()) {
                            details = l.get(0);
                        }
                    }
                });
    }

    /** Opens Google Play's purchase sheet. */
    public static void buy(final Activity a) {
        if (client == null || !connected || details == null) {
            message = "결제 정보를 불러오는 중이에요. 잠시 뒤에 다시 눌러 주세요";
            refresh(a);
            return;
        }
        final ProductDetails d = details;
        a.runOnUiThread(new Runnable() {
            public void run() {
                BillingFlowParams params = BillingFlowParams.newBuilder()
                        .setProductDetailsParamsList(Collections.singletonList(
                                BillingFlowParams.ProductDetailsParams.newBuilder().setProductDetails(d).build()))
                        .build();
                client.launchBillingFlow(a, params);
            }
        });
    }

    /** 0 = not ready (no Play, offline, not installed from Play), 1 = can buy, 2 = owned, 3 = payment pending. */
    public static int state() {
        if (owned) {
            return 2;
        }
        if (pending) {
            return 3;
        }
        return connected && details != null ? 1 : 0;
    }

    /** Price text in the person's currency (e.g. "₩9,900"), or "". */
    public static String price() {
        if (details == null || details.getOneTimePurchaseOfferDetails() == null) {
            return "";
        }
        return details.getOneTimePurchaseOfferDetails().getFormattedPrice();
    }

    /** A short message to show once (purchase done, failed), then empty. */
    public static String takeMessage() {
        String m = message;
        message = "";
        return m;
    }
}
