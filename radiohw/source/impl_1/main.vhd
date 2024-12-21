library IEEE;
use IEEE.STD_LOGIC_1164.ALL;
use IEEE.NUMERIC_STD.ALL;

entity radio is
   Generic(
        sim: std_logic := '0');
   Port (
		pcie_rx: in std_logic_vector(1 downto 0);
		pcie_tx: out std_logic_vector(1 downto 0);
		pcie_refclk: in std_logic_vector(1 downto 0);
		pcie_wake_n: in std_logic;
		pcie_perst_n: in std_logic;
		pcie_tck: in std_logic;
		pcie_tdi: in std_logic;
		pcie_tdo: out std_logic;
		pcie_tms: in std_logic;
		pcie_present_n: out std_logic;
		pcie_smbus_clk: inout std_logic;
		pcie_smbus_data: inout std_logic;
		pcie_rext: in std_logic; --?
		pcie_refret: in std_logic; --?
		pcie_aux_power: in std_logic;
		mipi_csi_a_d: in std_logic_vector(7 downto 0);
		mipi_csi_a_clk: in std_logic_vector(1 downto 0);
		gpio_a: in std_logic_vector(7 downto 0);
		i2c_a_scl: inout std_logic;
		i2c_a_sda: inout std_logic;
		ds90_inta: in std_logic;
		mipi_csi_b_d: in std_logic_vector(7 downto 0);
		mipi_csi_b_clk: in std_logic_vector(1 downto 0);
		gpio_b: in std_logic_vector(7 downto 0);
		i2c_b_scl: inout std_logic;
		i2c_b_sda: inout std_logic;
		ds90_intb: in std_logic;
		mipi_csi_c_d: in std_logic_vector(7 downto 0);
		mipi_csi_c_clk: in std_logic_vector(1 downto 0);
		gpio_c: in std_logic_vector(7 downto 0);
		i2c_c_scl: inout std_logic;
		i2c_c_sda: inout std_logic;
		ds90_intc: in std_logic;
		clock: in std_logic);
end radio;

architecture Behavioral of radio is
	signal reset: std_logic := '1';

	signal mipi_a_dp: std_logic_vector(3 downto 0);
	signal mipi_a_dn: std_logic_vector(3 downto 0);
	signal mipi_b_dp: std_logic_vector(3 downto 0);
	signal mipi_b_dn: std_logic_vector(3 downto 0);
	signal mipi_c_dp: std_logic_vector(3 downto 0);
	signal mipi_c_dn: std_logic_vector(3 downto 0);

	signal pcie_aux_clk: std_logic := '0';
	signal pcie_user_clk: std_logic := '0';
	signal pcie_phy_clk: std_logic;
	signal pcie_clkreq_n: std_logic;
	signal pcie_user_reset_n: std_logic := '1';
	signal pcie_status_phy: std_logic;
	signal pcie_status_data: std_logic;
	signal pcie_status_trans: std_logic;
	
	component mipi_csi_16_nx is
		generic(
			MIPI_LANES: in integer := 2;
			MIPI_GEAR: in integer := 8;
			MIPI_PIXEL_PER_CLOCK: in integer := 2;
			MAX_PIXEL_WIDTH: in integer := 12;
			FRAME_DETECT: in integer := 0);
		port(
			mipi_clk_p_in: in std_logic;
			mipi_clk_n_in: in std_logic;
			mipi_data_p_in: in std_logic_vector(3 downto 0);
			mipi_data_n_in: in std_logic_vector(3 downto 0);
			pclk_o: out std_logic;
			data_o: out std_logic;
			fsync_o: out std_logic;
			cam_xce_o: out std_logic;
			cam_pwr_en_o: out std_logic;
			cam_reset_o: out std_logic;
			cam_xmaster_o: out std_logic);
	end component;

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
	
	signal pcie_ahb1_addr: std_logic_vector(31 downto 0);
	signal pcie_ahb1_burst: std_logic_vector(2 downto 0);
	signal pcie_ahb1_size: std_logic_vector(2 downto 0);
	signal pcie_ahb1_type: std_logic_vector(1 downto 0);
	signal pcie_ahb1_data_in: std_logic_vector(31 downto 0);
	signal pcie_ahb1_data_out: std_logic_vector(31 downto 0);
	signal pcie_ahb1_ready: std_logic;
	signal pcie_ahb1_response: std_logic;
	signal pcie_ahb1_write: std_logic;
	
	signal pcie_ahb2_addr: std_logic_vector(31 downto 0);
	signal pcie_ahb2_burst: std_logic_vector(2 downto 0);
	signal pcie_ahb2_size: std_logic_vector(2 downto 0);
	signal pcie_ahb2_type: std_logic_vector(1 downto 0);
	signal pcie_ahb2_data_in: std_logic_vector(31 downto 0);
	signal pcie_ahb2_data_out: std_logic_vector(31 downto 0);
	signal pcie_ahb2_ready: std_logic;
	signal pcie_ahb2_response: std_logic;
	signal pcie_ahb2_write: std_logic;
	
	signal pcie_apb_clk: std_logic;
	signal pcie_apb_reset_n: std_logic;
	signal pcie_apb_addr: std_logic_vector(31 downto 0);
	signal pcie_apb_select: std_logic;
	signal pcie_apb_enable: std_logic;
	signal pcie_apb_write: std_logic;
	signal pcie_apb_dout: std_logic_vector(31 downto 0);
	signal pcie_apb_din: std_logic_vector(31 downto 0);
	signal pcie_apb_ready: std_logic;
	signal pcie_apb_err: std_logic;
	
	signal pcie_int1: std_logic;
	signal pcie_int2: std_logic;
	
	signal i2c_mclk: std_logic;
	
	signal pcie_transactions_pending: std_logic_vector(3 downto 0);
begin

	--TODO:
	--pcie_aux_clk at 16MHz
	--pcie_user_clk at 125MHz or higher for 5gbps pcie data
	--pcie_user_reset_n
	--reset
	
	por: entity work.por generic map(cycles => 10) port map(reset => reset, clock => clock);
	
	pcie_present_n <= reset;
	
	mipi_a_dp <= mipi_csi_a_d(6) & mipi_csi_a_d(4) & mipi_csi_a_d(2) & mipi_csi_a_d(0);
	mipi_a_dn <= mipi_csi_a_d(7) & mipi_csi_a_d(5) & mipi_csi_a_d(3) & mipi_csi_a_d(1);
	mipi_b_dp <= mipi_csi_b_d(6) & mipi_csi_b_d(4) & mipi_csi_b_d(2) & mipi_csi_b_d(0);
	mipi_b_dn <= mipi_csi_b_d(7) & mipi_csi_b_d(5) & mipi_csi_b_d(3) & mipi_csi_b_d(1);
	mipi_c_dp <= mipi_csi_c_d(6) & mipi_csi_c_d(4) & mipi_csi_c_d(2) & mipi_csi_c_d(0);
	mipi_c_dn <= mipi_csi_c_d(7) & mipi_csi_c_d(5) & mipi_csi_c_d(3) & mipi_csi_c_d(1);
	
	mipia: mipi_csi_16_nx generic map(
		MIPI_LANES => 4,
		MIPI_GEAR => 16,
		MIPI_PIXEL_PER_CLOCK => 4) port map(
		mipi_data_p_in => mipi_a_dp,
		mipi_data_n_in => mipi_a_dn,
		mipi_clk_p_in => mipi_csi_a_clk(0),
		mipi_clk_n_in => mipi_csi_a_clk(1));
	
	mipib: mipi_csi_16_nx generic map(
		MIPI_LANES => 4,
		MIPI_GEAR => 16,
		MIPI_PIXEL_PER_CLOCK => 4) port map(
		mipi_data_p_in => mipi_b_dp,
		mipi_data_n_in => mipi_b_dn,
		mipi_clk_p_in => mipi_csi_b_clk(0),
		mipi_clk_n_in => mipi_csi_b_clk(1));
	
	mipic: mipi_csi_16_nx generic map(
		MIPI_LANES => 4,
		MIPI_GEAR => 16,
		MIPI_PIXEL_PER_CLOCK => 4) port map(
		mipi_data_p_in => mipi_c_dp,
		mipi_data_n_in => mipi_c_dn,
		mipi_clk_p_in => mipi_csi_c_clk(0),
		mipi_clk_n_in => mipi_csi_c_clk(1));
	
	pcie_gen: if sim = '0' generate
		pcie: pcie_1x port map(
			rxp_i => pcie_rx(0),
			rxn_i => pcie_rx(1),
			refclkp_i => pcie_refclk(0),
			refclkn_i => pcie_refclk(1),
			aux_clk_i => pcie_aux_clk,
			txp_o => pcie_tx(0),
			txn_o => pcie_tx(1),
			refret_i=> pcie_refret,
			rext_i => pcie_rext,
			perst_n_i=> pcie_perst_n,
			rst_usr_n_i => pcie_user_reset_n,
			clk_usr_i => pcie_user_clk,
			clk_usr_o => pcie_phy_clk,
			u_pl_link_up_o => pcie_status_phy,
			u_dl_link_up_o => pcie_status_data,
			u_tl_link_up_o => pcie_status_trans,
			m_w_hready_i => pcie_ahb1_ready,
			m_w_hresp_i => pcie_ahb1_response,
			m_w_hrdata_i => pcie_ahb1_data_out,
			m_w_haddr_o => pcie_ahb1_addr,
			m_w_hburst_o => pcie_ahb1_burst,
			m_w_hsize_o => pcie_ahb1_size,
			m_w_htrans_o => pcie_ahb1_type,
			m_w_hwrite_o => pcie_ahb1_write,
			m_w_hwdata_o => pcie_ahb1_data_in,
			m_r_hready_i => pcie_ahb2_ready,
			m_r_hresp_i => pcie_ahb2_response,
			m_r_hrdata_i => pcie_ahb2_data_out,
			m_r_haddr_o => pcie_ahb2_addr,
			m_r_hburst_o => pcie_ahb2_burst,
			m_r_hsize_o => pcie_ahb2_size,
			m_r_htrans_o => pcie_ahb2_type,
			m_r_hwrite_o => pcie_ahb2_write,
			m_r_hwdata_o => pcie_ahb2_data_in,
			c_apb_pclk_i => pcie_apb_clk,
			c_apb_preset_n_i => pcie_apb_reset_n,
			c_apb_paddr_i => pcie_apb_addr,
			c_apb_psel_i => pcie_apb_select,
			c_apb_penable_i => pcie_apb_enable,
			c_apb_pwrite_i => pcie_apb_write,
			c_apb_pwdata_i => pcie_apb_dout,
			c_apb_prdata_o => pcie_apb_din,
			c_apb_pready_o => pcie_apb_ready,
			c_apb_pslverr_o => pcie_apb_err,
			int_normal_o => pcie_int1,
			int_critical_o => pcie_int2,
			user_aux_power_detected_i => pcie_aux_power,
			user_transactions_pending_i=> pcie_transactions_pending);
		end generate;

end Behavioral;

