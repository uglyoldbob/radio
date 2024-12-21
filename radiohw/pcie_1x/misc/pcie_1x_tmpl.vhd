component pcie_1x is
    port(
        rxp_i: in std_logic;
        rxn_i: in std_logic;
        refclkp_i: in std_logic;
        refclkn_i: in std_logic;
        aux_clk_i: in std_logic;
        txp_o: out std_logic;
        txn_o: out std_logic;
        refret_i: in std_logic;
        rext_i: in std_logic;
        perst_n_i: in std_logic;
        rst_usr_n_i: in std_logic;
        clk_usr_i: in std_logic;
        clk_usr_o: out std_logic;
        u_pl_link_up_o: out std_logic;
        u_dl_link_up_o: out std_logic;
        u_tl_link_up_o: out std_logic;
        m_w_hready_i: in std_logic;
        m_w_hresp_i: in std_logic;
        m_w_hrdata_i: in std_logic_vector(31 downto 0);
        m_w_haddr_o: out std_logic_vector(31 downto 0);
        m_w_hburst_o: out std_logic_vector(2 downto 0);
        m_w_hmastlock_o: out std_logic;
        m_w_hprot_o: out std_logic_vector(3 downto 0);
        m_w_hsize_o: out std_logic_vector(2 downto 0);
        m_w_htrans_o: out std_logic_vector(1 downto 0);
        m_w_hwrite_o: out std_logic;
        m_w_hwdata_o: out std_logic_vector(31 downto 0);
        m_r_hready_i: in std_logic;
        m_r_hresp_i: in std_logic;
        m_r_hrdata_i: in std_logic_vector(31 downto 0);
        m_r_haddr_o: out std_logic_vector(31 downto 0);
        m_r_hburst_o: out std_logic_vector(2 downto 0);
        m_r_hmastlock_o: out std_logic;
        m_r_hprot_o: out std_logic_vector(3 downto 0);
        m_r_hsize_o: out std_logic_vector(2 downto 0);
        m_r_htrans_o: out std_logic_vector(1 downto 0);
        m_r_hwrite_o: out std_logic;
        m_r_hwdata_o: out std_logic_vector(31 downto 0);
        c_apb_pclk_i: in std_logic;
        c_apb_preset_n_i: in std_logic;
        c_apb_paddr_i: in std_logic_vector(31 downto 0);
        c_apb_psel_i: in std_logic;
        c_apb_penable_i: in std_logic;
        c_apb_pwrite_i: in std_logic;
        c_apb_pwdata_i: in std_logic_vector(31 downto 0);
        c_apb_prdata_o: out std_logic_vector(31 downto 0);
        c_apb_pready_o: out std_logic;
        c_apb_pslverr_o: out std_logic;
        int_normal_o: out std_logic;
        int_critical_o: out std_logic;
        user_aux_power_detected_i: in std_logic;
        user_transactions_pending_i: in std_logic_vector(3 downto 0)
    );
end component;

__: pcie_1x port map(
    rxp_i=>,
    rxn_i=>,
    refclkp_i=>,
    refclkn_i=>,
    aux_clk_i=>,
    txp_o=>,
    txn_o=>,
    refret_i=>,
    rext_i=>,
    perst_n_i=>,
    rst_usr_n_i=>,
    clk_usr_i=>,
    clk_usr_o=>,
    u_pl_link_up_o=>,
    u_dl_link_up_o=>,
    u_tl_link_up_o=>,
    m_w_hready_i=>,
    m_w_hresp_i=>,
    m_w_hrdata_i=>,
    m_w_haddr_o=>,
    m_w_hburst_o=>,
    m_w_hmastlock_o=>,
    m_w_hprot_o=>,
    m_w_hsize_o=>,
    m_w_htrans_o=>,
    m_w_hwrite_o=>,
    m_w_hwdata_o=>,
    m_r_hready_i=>,
    m_r_hresp_i=>,
    m_r_hrdata_i=>,
    m_r_haddr_o=>,
    m_r_hburst_o=>,
    m_r_hmastlock_o=>,
    m_r_hprot_o=>,
    m_r_hsize_o=>,
    m_r_htrans_o=>,
    m_r_hwrite_o=>,
    m_r_hwdata_o=>,
    c_apb_pclk_i=>,
    c_apb_preset_n_i=>,
    c_apb_paddr_i=>,
    c_apb_psel_i=>,
    c_apb_penable_i=>,
    c_apb_pwrite_i=>,
    c_apb_pwdata_i=>,
    c_apb_prdata_o=>,
    c_apb_pready_o=>,
    c_apb_pslverr_o=>,
    int_normal_o=>,
    int_critical_o=>,
    user_aux_power_detected_i=>,
    user_transactions_pending_i=>
);
